//! Broker: publish/subscribe + replay + backpressure dispatch + groups + acks.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::bridge::Bridge;
use crate::core::envelope::Envelope;
use crate::core::error::{EventMeshError, Result};
use crate::store::{EventStore, ReplayFilter};
use crate::core::topic::{Topic, TopicPattern};

use super::dedup::DedupRing;
use crate::ext::filter::Filter;
use super::ring;
use super::subscriber::{
    AckMode, BackpressurePolicy, GroupStrategy, Requeue, SenderHandle, SubId, SubReceiver,
    SubSender, SubscribeOptions, Subscriber, Subscription,
};

pub const DEFAULT_BUFFER: usize = 1024;
pub const DEFAULT_DEDUP_WINDOW: usize = 4096;

/// Round-robin pointer + member list for a consumer group.
pub(super) struct GroupState {
    pub members: Vec<Arc<Subscriber>>,
    pub next: AtomicUsize,
    pub strategy: GroupStrategy,
}

/// Tracking entry for an in-flight (delivered, not yet acked) envelope.
#[allow(dead_code)]
struct Inflight {
    env: Arc<Envelope>,
    sub_id: SubId,
    group: Option<String>,
    pattern: TopicPattern,
    visibility: Duration,
    max_attempts: u32,
    dlq_topic: Option<String>,
    deadline: Instant,
    attempts: u32,
}

pub(super) struct Inner {
    next_sub_id: AtomicU64,
    /// Ungrouped subscribers (classic fan-out: every match gets a copy).
    pub(super) fanout_subs: RwLock<Vec<Arc<Subscriber>>>,
    /// Consumer groups by name (one envelope dispatched to a single member).
    pub(super) groups: RwLock<HashMap<String, GroupState>>,
    dedup: Mutex<DedupRing>,
    closed: AtomicBool,
    store: Option<Arc<dyn EventStore>>,
    bridges: RwLock<Vec<Arc<dyn Bridge>>>,
    /// Inflight envelopes awaiting ack. Keyed by (sub, env_id).
    inflight: Mutex<HashMap<(SubId, Uuid), Inflight>>,
    /// Whether the redelivery reaper has been spawned for this mesh.
    reaper_started: AtomicBool,
}

#[derive(Clone)]
pub struct EventMesh {
    inner: Arc<Inner>,
}

pub struct EventMeshBuilder {
    dedup_window: usize,
    store: Option<Arc<dyn EventStore>>,
}

impl EventMeshBuilder {
    pub fn dedup_window(mut self, n: usize) -> Self {
        self.dedup_window = n;
        self
    }
    pub fn store(mut self, store: Arc<dyn EventStore>) -> Self {
        self.store = Some(store);
        self
    }
    pub fn build(self) -> EventMesh {
        EventMesh {
            inner: Arc::new(Inner {
                next_sub_id: AtomicU64::new(1),
                fanout_subs: RwLock::new(Vec::new()),
                groups: RwLock::new(HashMap::new()),
                dedup: Mutex::new(DedupRing::new(self.dedup_window)),
                closed: AtomicBool::new(false),
                store: self.store,
                bridges: RwLock::new(Vec::new()),
                inflight: Mutex::new(HashMap::new()),
                reaper_started: AtomicBool::new(false),
            }),
        }
    }
}

impl EventMesh {
    pub fn new() -> Self {
        Self::builder().build()
    }
    pub fn builder() -> EventMeshBuilder {
        EventMeshBuilder {
            dedup_window: DEFAULT_DEDUP_WINDOW,
            store: None,
        }
    }
    pub fn with_store(store: Arc<dyn EventStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub fn subscribe(&self, pattern: TopicPattern) -> Subscription {
        self.subscribe_with(pattern, SubscribeOptions::default())
    }
    pub fn subscribe_with_buffer(&self, pattern: TopicPattern, buffer: usize) -> Subscription {
        self.subscribe_with(pattern, SubscribeOptions::default().buffer(buffer))
    }

    pub fn subscribe_with(
        &self,
        pattern: TopicPattern,
        opts: SubscribeOptions,
    ) -> Subscription {
        let opts = opts.with_default_buffer();
        let id = SubId(self.inner.next_sub_id.fetch_add(1, Ordering::Relaxed));

        let (sender, rx) = match opts.policy {
            BackpressurePolicy::DropOldest => {
                let (tx, rx) = ring::channel(opts.buffer);
                (SubSender::Ring(tx), SubReceiver::Ring(rx))
            }
            _ => {
                let (tx, rx) = mpsc::channel(opts.buffer);
                (SubSender::Bounded(tx), SubReceiver::Bounded(rx))
            }
        };

        let sub = Arc::new(Subscriber {
            id,
            pattern: pattern.clone(),
            sender,
            policy: opts.policy,
            filter: opts.filter.clone(),
            group: opts.group.clone(),
            ack: opts.ack.clone(),
        });

        if let Some(group_name) = &opts.group {
            let mut groups = self.inner.groups.write();
            let entry = groups
                .entry(group_name.clone())
                .or_insert_with(|| GroupState {
                    members: Vec::new(),
                    next: AtomicUsize::new(0),
                    strategy: opts.group_strategy.clone(),
                });
            entry.members.push(sub);
        } else {
            self.inner.fanout_subs.write().push(sub);
        }

        // Lazy-start the redelivery reaper if any explicit-ack sub may exist.
        if matches!(opts.ack, AckMode::Explicit { .. })
            && !self.inner.reaper_started.swap(true, Ordering::AcqRel)
        {
            self.spawn_reaper();
        }

        Subscription {
            rx,
            pattern,
            policy: opts.policy,
            id,
            inner: Arc::downgrade(&self.inner),
            group: opts.group,
        }
    }

    pub fn subscribe_str(&self, pattern: &str) -> Result<Subscription> {
        Ok(self.subscribe(TopicPattern::new(pattern)?))
    }

    /// Convenience: subscribe with a closure filter.
    pub fn subscribe_where<F>(
        &self,
        pattern: TopicPattern,
        predicate: F,
    ) -> Subscription
    where
        F: Fn(&Envelope) -> bool + Send + Sync + 'static,
    {
        self.subscribe_with(
            pattern,
            SubscribeOptions::default().filter(Filter::predicate(predicate)),
        )
    }

    pub async fn publish(&self, env: Envelope) -> Result<usize> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(EventMeshError::Closed);
        }
        if env.is_expired() {
            return Ok(0);
        }
        if let Some(key) = &env.idempotency_key {
            if self.inner.dedup.lock().check_and_insert(&env.topic, key) {
                return Ok(0);
            }
        }

        Topic::validate_str(&env.topic)?;
        let env = Arc::new(env);

        if let Some(store) = &self.inner.store {
            store.append(env.clone()).await?;
        }

        let bridges: Vec<Arc<dyn Bridge>> = self.inner.bridges.read().clone();
        for b in &bridges {
            let _ = b.publish_remote(env.clone()).await;
        }

        self.fanout(&env).await
    }

    pub async fn emit(&self, topic: &Topic, payload: serde_json::Value) -> Result<usize> {
        self.publish(Envelope::new(topic, payload)).await
    }

    async fn fanout(&self, env: &Arc<Envelope>) -> Result<usize> {
        let topic_segs: Vec<&str> = env.topic.split('.').collect();

        // 1) Snapshot ungrouped matching subscribers.
        let direct: Vec<Arc<Subscriber>> = {
            let subs = self.inner.fanout_subs.read();
            subs.iter()
                .filter(|s| s.pattern.matches_segments(&topic_segs))
                .cloned()
                .collect()
        };

        // 2) Snapshot one chosen member per matching group.
        let group_targets: Vec<Arc<Subscriber>> = {
            let groups = self.inner.groups.read();
            let mut chosen = Vec::new();
            for state in groups.values() {
                let matching: Vec<&Arc<Subscriber>> = state
                    .members
                    .iter()
                    .filter(|m| m.pattern.matches_segments(&topic_segs))
                    .collect();
                if matching.is_empty() {
                    continue;
                }
                let pick = match &state.strategy {
                    GroupStrategy::RoundRobin => {
                        let i = state.next.fetch_add(1, Ordering::Relaxed);
                        matching[i % matching.len()].clone()
                    }
                    GroupStrategy::KeyHash { header_key } => {
                        let key = env.headers.get(header_key);
                        let idx = match key {
                            Some(k) => {
                                let mut h: u64 = 1469598103934665603; // FNV-1a 64
                                for b in k.as_bytes() {
                                    h ^= *b as u64;
                                    h = h.wrapping_mul(1099511628211);
                                }
                                (h as usize) % matching.len()
                            }
                            None => {
                                let i = state.next.fetch_add(1, Ordering::Relaxed);
                                i % matching.len()
                            }
                        };
                        matching[idx].clone()
                    }
                };
                chosen.push(pick);
            }
            chosen
        };

        let mut delivered = 0usize;
        let mut had_dead_direct = false;
        let mut had_dead_groups = false;

        for s in direct.iter().chain(group_targets.iter()) {
            // Server-side filter prune (avoid wasting buffer slots).
            if let Some(f) = &s.filter {
                if !f.matches(env) {
                    continue;
                }
            }
            let outcome = self.deliver_to(s, env.clone(), 1).await;
            match outcome {
                DeliveryOutcome::Delivered => delivered += 1,
                DeliveryOutcome::Dropped => {}
                DeliveryOutcome::Closed => {
                    if s.group.is_some() {
                        had_dead_groups = true;
                    } else {
                        had_dead_direct = true;
                    }
                }
            }
        }

        if had_dead_direct {
            self.inner
                .fanout_subs
                .write()
                .retain(|s| !s.sender.is_closed());
        }
        if had_dead_groups {
            let mut groups = self.inner.groups.write();
            groups.retain(|_, state| {
                state.members.retain(|s| !s.sender.is_closed());
                !state.members.is_empty()
            });
        }
        Ok(delivered)
    }

    async fn deliver_to(
        &self,
        sub: &Arc<Subscriber>,
        env: Arc<Envelope>,
        attempt: u32,
    ) -> DeliveryOutcome {
        // Track inflight up-front for explicit-ack subs.
        if let AckMode::Explicit {
            visibility,
            max_attempts,
            dlq_topic,
        } = &sub.ack
        {
            let mut inflight = self.inner.inflight.lock();
            inflight.insert(
                (sub.id, env.id),
                Inflight {
                    env: env.clone(),
                    sub_id: sub.id,
                    group: sub.group.clone(),
                    pattern: sub.pattern.clone(),
                    visibility: *visibility,
                    max_attempts: *max_attempts,
                    dlq_topic: dlq_topic.clone(),
                    deadline: Instant::now() + *visibility,
                    attempts: attempt,
                },
            );
        }

        let handle = SenderHandle::from(&sub.sender);
        match handle {
            SenderHandle::Bounded(tx) => match sub.policy {
                BackpressurePolicy::Block => {
                    if tx.send(env).await.is_ok() {
                        DeliveryOutcome::Delivered
                    } else {
                        DeliveryOutcome::Closed
                    }
                }
                BackpressurePolicy::DropNewest => match tx.try_send(env) {
                    Ok(_) => DeliveryOutcome::Delivered,
                    Err(mpsc::error::TrySendError::Full(_)) => DeliveryOutcome::Dropped,
                    Err(mpsc::error::TrySendError::Closed(_)) => DeliveryOutcome::Closed,
                },
                BackpressurePolicy::Disconnect => match tx.try_send(env) {
                    Ok(_) => DeliveryOutcome::Delivered,
                    Err(_) => DeliveryOutcome::Closed,
                },
                BackpressurePolicy::DropOldest => unreachable!(),
            },
            SenderHandle::Ring(tx) => match tx.send(env) {
                ring::SendOutcome::Enqueued | ring::SendOutcome::EvictedOldest => {
                    DeliveryOutcome::Delivered
                }
                ring::SendOutcome::Closed => DeliveryOutcome::Closed,
            },
        }
    }

    /// Replay events from the attached store, optionally re-delivering.
    pub async fn replay(
        &self,
        filter: ReplayFilter,
        redeliver: bool,
    ) -> Result<Vec<Arc<Envelope>>> {
        let store = self
            .inner
            .store
            .as_ref()
            .ok_or(EventMeshError::NoStore)?
            .clone();
        let events = store.query(&filter).await?;
        if redeliver {
            for env in &events {
                let _ = self.fanout(env).await;
            }
        }
        Ok(events)
    }

    pub fn store(&self) -> Option<Arc<dyn EventStore>> {
        self.inner.store.clone()
    }

    pub fn attach_bridge(&self, bridge: Arc<dyn Bridge>) {
        self.inner.bridges.write().push(bridge);
    }

    pub async fn ingest_remote(&self, env: Arc<Envelope>) -> Result<usize> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(EventMeshError::Closed);
        }
        if env.is_expired() {
            return Ok(0);
        }
        let key = env.id.to_string();
        if self.inner.dedup.lock().check_and_insert(&env.topic, &key) {
            return Ok(0);
        }
        self.fanout(&env).await
    }

    pub fn subscriber_count(&self) -> usize {
        let direct = self.inner.fanout_subs.read().len();
        let grouped: usize = self
            .inner
            .groups
            .read()
            .values()
            .map(|g| g.members.len())
            .sum();
        direct + grouped
    }

    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
        self.inner.fanout_subs.write().clear();
        self.inner.groups.write().clear();
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.inner.closed.store(true, Ordering::Release);
        if let Some(store) = self.inner.store.as_ref().cloned() {
            store.flush().await?;
            store.shutdown().await?;
        }
        self.inner.fanout_subs.write().clear();
        self.inner.groups.write().clear();
        self.inner.inflight.lock().clear();
        Ok(())
    }

    /// Spawn the redelivery reaper. Idempotent: only the first explicit-ack
    /// subscribe triggers it (see `subscribe_with`).
    fn spawn_reaper(&self) {
        let weak = Arc::downgrade(&self.inner);
        let me = self.clone();
        tokio::spawn(async move {
            let tick = Duration::from_millis(200);
            loop {
                tokio::time::sleep(tick).await;
                let inner = match weak.upgrade() {
                    Some(i) => i,
                    None => return,
                };
                if inner.closed.load(Ordering::Acquire) {
                    return;
                }
                let now = Instant::now();
                let expired: Vec<((SubId, Uuid), Inflight)> = {
                    let mut map = inner.inflight.lock();
                    let keys: Vec<(SubId, Uuid)> = map
                        .iter()
                        .filter(|(_, v)| v.deadline <= now)
                        .map(|(k, _)| *k)
                        .collect();
                    keys.into_iter()
                        .filter_map(|k| map.remove_entry(&k))
                        .collect()
                };
                drop(inner);
                for (_, info) in expired {
                    me.handle_redelivery(info).await;
                }
            }
        });
    }

    async fn handle_redelivery(&self, info: Inflight) {
        if info.attempts >= info.max_attempts {
            self.send_to_dlq(info).await;
            return;
        }
        let next_attempt = info.attempts.saturating_add(1);
        // Try to find the original subscriber; if it's gone, dispatch to its
        // group (if any) — that's fine because group means any member can
        // process. For ungrouped, we just give up.
        let sub = self.find_subscriber(info.sub_id, info.group.as_deref());
        match sub {
            Some(s) => {
                let _ = self.deliver_to(&s, info.env.clone(), next_attempt).await;
            }
            None => {
                if let Some(group_name) = &info.group {
                    self.redispatch_to_group(group_name, info.env.clone(), next_attempt)
                        .await;
                }
            }
        }
    }

    fn find_subscriber(&self, id: SubId, group: Option<&str>) -> Option<Arc<Subscriber>> {
        if let Some(g) = group {
            let groups = self.inner.groups.read();
            if let Some(state) = groups.get(g) {
                if let Some(s) = state.members.iter().find(|m| m.id == id) {
                    return Some(s.clone());
                }
            }
            None
        } else {
            self.inner
                .fanout_subs
                .read()
                .iter()
                .find(|s| s.id == id)
                .cloned()
        }
    }

    async fn redispatch_to_group(&self, group: &str, env: Arc<Envelope>, attempt: u32) {
        let topic_segs: Vec<&str> = env.topic.split('.').collect();
        let pick = {
            let groups = self.inner.groups.read();
            let state = match groups.get(group) {
                Some(s) => s,
                None => return,
            };
            let matching: Vec<&Arc<Subscriber>> = state
                .members
                .iter()
                .filter(|m| m.pattern.matches_segments(&topic_segs))
                .collect();
            if matching.is_empty() {
                return;
            }
            let i = state.next.fetch_add(1, Ordering::Relaxed);
            matching[i % matching.len()].clone()
        };
        let _ = self.deliver_to(&pick, env, attempt).await;
    }

    async fn send_to_dlq(&self, info: Inflight) {
        let Inflight {
            env, dlq_topic, ..
        } = info;
        let Some(dlq) = dlq_topic else {
            return;
        };
        let Ok(topic) = Topic::new(dlq) else {
            return;
        };
        let payload = serde_json::json!({
            "original_topic": env.topic,
            "original_id": env.id,
            "payload": env.payload,
        });
        let mut dlq_env = Envelope::new(&topic, payload);
        dlq_env.headers.insert("dlq.original_topic".into(), env.topic.clone());
        dlq_env.headers.insert("dlq.original_id".into(), env.id.to_string());
        let _ = self.publish(dlq_env).await;
    }

    /// Snapshot inflight count (testing / metrics).
    pub fn inflight_count(&self) -> usize {
        self.inner.inflight.lock().len()
    }
}

enum DeliveryOutcome {
    Delivered,
    Dropped,
    Closed,
}

impl Default for EventMesh {
    fn default() -> Self {
        Self::new()
    }
}

// ---- crate-internal hooks called from `Subscription` ----------------------

pub(super) fn detach_subscriber(inner: &Arc<Inner>, id: SubId, group: Option<&str>) {
    if let Some(g) = group {
        let mut groups = inner.groups.write();
        if let Some(state) = groups.get_mut(g) {
            state.members.retain(|m| m.id != id);
        }
        groups.retain(|_, s| !s.members.is_empty());
    } else {
        inner.fanout_subs.write().retain(|s| s.id != id);
    }
    // Drop any inflight for this sub.
    inner.inflight.lock().retain(|(sid, _), _| *sid != id);
}

pub(super) fn ack_envelope(inner: &Arc<Inner>, sub: SubId, env_id: Uuid) {
    inner.inflight.lock().remove(&(sub, env_id));
}

pub(super) fn nack_envelope(
    inner: &Arc<Inner>,
    sub: SubId,
    env_id: Uuid,
    mode: Requeue,
) {
    let entry = inner.inflight.lock().remove(&(sub, env_id));
    let Some(info) = entry else {
        return;
    };
    // We don't have an EventMesh handle here, but we can clone via a wrapper.
    // Spawn an async task that drives redelivery using a temporary EventMesh
    // wrapping the same `Arc<Inner>`.
    let mesh = EventMesh { inner: inner.clone() };
    tokio::spawn(async move {
        match mode {
            Requeue::Immediate => mesh.handle_redelivery(info).await,
            Requeue::Delay(d) => {
                tokio::time::sleep(d).await;
                mesh.handle_redelivery(info).await;
            }
            Requeue::Dead => mesh.send_to_dlq(info).await,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use serde_json::json;

    #[tokio::test]
    async fn pub_sub_exact() {
        let mesh = EventMesh::new();
        let mut sub = mesh.subscribe_str("flow.order.created").unwrap();
        let topic = Topic::new("flow.order.created").unwrap();
        let n = mesh.emit(&topic, json!({"id": 1})).await.unwrap();
        assert_eq!(n, 1);
        let env = sub.recv().await.unwrap();
        assert_eq!(env.topic, "flow.order.created");
        assert_eq!(env.payload["id"], 1);
    }

    #[tokio::test]
    async fn wildcard() {
        let mesh = EventMesh::new();
        let mut a = mesh.subscribe_str("flow.*.created").unwrap();
        let mut b = mesh.subscribe_str("flow.>").unwrap();
        let topic = Topic::new("flow.order.created").unwrap();
        let n = mesh.emit(&topic, json!({})).await.unwrap();
        assert_eq!(n, 2);
        assert!(a.recv().await.is_some());
        assert!(b.recv().await.is_some());
    }

    #[tokio::test]
    async fn idempotency() {
        let mesh = EventMesh::new();
        let mut sub = mesh.subscribe_str("x.>").unwrap();
        let topic = Topic::new("x.y").unwrap();
        let env = Envelope::new(&topic, json!({})).with_idempotency_key("k1");
        let env2 = Envelope::new(&topic, json!({})).with_idempotency_key("k1");
        assert_eq!(mesh.publish(env).await.unwrap(), 1);
        assert_eq!(mesh.publish(env2).await.unwrap(), 0);
        assert!(sub.recv().await.is_some());
        assert!(sub.try_recv().is_err());
    }

    #[tokio::test]
    async fn replay_with_memory_store() {
        let store = Arc::new(MemoryStore::new());
        let mesh = EventMesh::with_store(store.clone());
        let t = Topic::new("a.b").unwrap();
        mesh.emit(&t, json!({"n": 1})).await.unwrap();
        mesh.emit(&t, json!({"n": 2})).await.unwrap();
        let mut sub = mesh.subscribe_str("a.>").unwrap();
        let got = mesh
            .replay(ReplayFilter::topic("a.>").unwrap(), true)
            .await
            .unwrap();
        assert_eq!(got.len(), 2);
        let r1 = sub.recv().await.unwrap();
        let r2 = sub.recv().await.unwrap();
        assert_eq!(r1.payload["n"], 1);
        assert_eq!(r2.payload["n"], 2);
    }

    #[tokio::test]
    async fn drop_newest_when_full() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("z.>").unwrap();
        let mut sub = mesh.subscribe_with(
            pat,
            SubscribeOptions::default()
                .buffer(2)
                .policy(BackpressurePolicy::DropNewest),
        );
        let t = Topic::new("z.k").unwrap();
        for i in 0..10u32 {
            mesh.emit(&t, json!({ "i": i })).await.unwrap();
        }
        let mut got = 0;
        while sub.try_recv().is_ok() {
            got += 1;
        }
        assert!(got <= 2, "got={got}");
    }

    #[tokio::test]
    async fn drop_oldest_keeps_newest() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("zo.>").unwrap();
        let mut sub = mesh.subscribe_with(
            pat,
            SubscribeOptions::default()
                .buffer(3)
                .policy(BackpressurePolicy::DropOldest),
        );
        let t = Topic::new("zo.k").unwrap();
        for i in 0..5u32 {
            mesh.emit(&t, json!({ "i": i })).await.unwrap();
        }
        let mut got = Vec::new();
        while let Ok(env) = sub.try_recv() {
            got.push(env.payload["i"].as_u64().unwrap() as u32);
        }
        assert_eq!(got, vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn channel_bridge_federates_two_meshes() {
        use crate::bridge::ChannelBridgePair;

        let mesh_a = EventMesh::new();
        let mesh_b = EventMesh::new();
        let pair = ChannelBridgePair::new();
        mesh_a.attach_bridge(pair.a_to_b.clone());
        mesh_b.attach_bridge(pair.b_to_a.clone());

        let mesh_b_clone = mesh_b.clone();
        let mut b_inbox = pair.b_inbox;
        let pump_b = tokio::spawn(async move {
            while let Some(env) = b_inbox.recv().await {
                let _ = mesh_b_clone.ingest_remote(env).await;
            }
        });
        let mesh_a_clone = mesh_a.clone();
        let mut a_inbox = pair.a_inbox;
        let pump_a = tokio::spawn(async move {
            while let Some(env) = a_inbox.recv().await {
                let _ = mesh_a_clone.ingest_remote(env).await;
            }
        });

        let mut sub_b = mesh_b.subscribe_str("xchain.>").unwrap();
        let t = Topic::new("xchain.hello").unwrap();
        mesh_a.emit(&t, json!({"v": 1})).await.unwrap();

        let env = tokio::time::timeout(std::time::Duration::from_secs(1), sub_b.recv())
            .await
            .expect("bridge delivery timed out")
            .unwrap();
        assert_eq!(env.topic, "xchain.hello");
        assert_eq!(env.payload["v"], 1);

        mesh_a.shutdown().await.unwrap();
        mesh_b.shutdown().await.unwrap();
        drop(pair.a_to_b);
        drop(pair.b_to_a);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), pump_a).await;
        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), pump_b).await;
    }

    // ---- new feature tests ------------------------------------------------

    #[tokio::test]
    async fn server_side_filter_prunes() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("orders.>").unwrap();
        let mut sub = mesh.subscribe_with(
            pat,
            SubscribeOptions::default().filter(Filter::header("region", "us")),
        );
        let t = Topic::new("orders.created").unwrap();
        let mut e1 = Envelope::new(&t, json!({"id": 1}));
        e1.headers.insert("region".into(), "us".into());
        let mut e2 = Envelope::new(&t, json!({"id": 2}));
        e2.headers.insert("region".into(), "eu".into());
        mesh.publish(e1).await.unwrap();
        mesh.publish(e2).await.unwrap();
        let got = sub.recv().await.unwrap();
        assert_eq!(got.payload["id"], 1);
        assert!(sub.try_recv().is_err());
    }

    #[tokio::test]
    async fn poll_batch() {
        let mesh = EventMesh::new();
        let mut sub = mesh.subscribe_str("p.>").unwrap();
        let t = Topic::new("p.a").unwrap();
        for i in 0..5u32 {
            mesh.emit(&t, json!({"i": i})).await.unwrap();
        }
        let batch = sub.poll(10, Duration::from_millis(50)).await;
        assert_eq!(batch.len(), 5);
    }

    #[tokio::test]
    async fn poll_timeout_empty() {
        let mesh = EventMesh::new();
        let mut sub = mesh.subscribe_str("p.>").unwrap();
        let batch = sub.poll(10, Duration::from_millis(20)).await;
        assert!(batch.is_empty());
    }

    #[tokio::test]
    async fn consumer_group_round_robin() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("g.>").unwrap();
        let mut a = mesh.subscribe_with(
            pat.clone(),
            SubscribeOptions::default().group("workers"),
        );
        let mut b = mesh.subscribe_with(
            pat,
            SubscribeOptions::default().group("workers"),
        );
        let t = Topic::new("g.task").unwrap();
        for i in 0..6u32 {
            mesh.emit(&t, json!({"i": i})).await.unwrap();
        }
        let mut a_got = 0;
        let mut b_got = 0;
        while let Ok(_) = a.try_recv() {
            a_got += 1;
        }
        while let Ok(_) = b.try_recv() {
            b_got += 1;
        }
        // Each event delivered to exactly one member.
        assert_eq!(a_got + b_got, 6);
        assert!(a_got > 0 && b_got > 0, "both should get some");
    }

    #[tokio::test]
    async fn consumer_group_keyhash_sticky() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("kh.>").unwrap();
        let mut a = mesh.subscribe_with(
            pat.clone(),
            SubscribeOptions::default()
                .group("workers")
                .group_strategy(GroupStrategy::KeyHash {
                    header_key: "k".into(),
                }),
        );
        let mut b = mesh.subscribe_with(
            pat,
            SubscribeOptions::default()
                .group("workers")
                .group_strategy(GroupStrategy::KeyHash {
                    header_key: "k".into(),
                }),
        );
        let t = Topic::new("kh.task").unwrap();
        // Send 10 events with the same key — must all land on the same member.
        for i in 0..10u32 {
            let mut e = Envelope::new(&t, json!({"i": i}));
            e.headers.insert("k".into(), "stable".into());
            mesh.publish(e).await.unwrap();
        }
        let mut a_got = 0;
        let mut b_got = 0;
        while let Ok(_) = a.try_recv() {
            a_got += 1;
        }
        while let Ok(_) = b.try_recv() {
            b_got += 1;
        }
        assert_eq!(a_got + b_got, 10);
        assert!(a_got == 10 || b_got == 10, "all same key → one member");
    }

    #[tokio::test]
    async fn ack_redelivery_on_timeout() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("ack.>").unwrap();
        let mut sub = mesh.subscribe_with(
            pat,
            SubscribeOptions::default()
                .group("workers")
                .ack(AckMode::explicit()
                    .with_visibility(Duration::from_millis(150))
                    .with_max_attempts(3)),
        );
        let t = Topic::new("ack.task").unwrap();
        mesh.emit(&t, json!({"v": 1})).await.unwrap();
        let first = sub.recv().await.unwrap();
        // Don't ack — wait for redelivery.
        let second = tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("redelivery timed out")
            .unwrap();
        assert_eq!(first.id, second.id);
        sub.ack(second.id).unwrap();
        // No more redeliveries after ack.
        let again = tokio::time::timeout(Duration::from_millis(400), sub.recv()).await;
        assert!(again.is_err(), "should not redeliver after ack");
    }

    #[tokio::test]
    async fn nack_immediate_requeues() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("nk.>").unwrap();
        let mut sub = mesh.subscribe_with(
            pat,
            SubscribeOptions::default()
                .group("workers")
                .ack(AckMode::explicit()
                    .with_visibility(Duration::from_secs(60))
                    .with_max_attempts(5)),
        );
        let t = Topic::new("nk.task").unwrap();
        mesh.emit(&t, json!({"v": 1})).await.unwrap();
        let first = sub.recv().await.unwrap();
        sub.nack(first.id, Requeue::Immediate).unwrap();
        let second = tokio::time::timeout(Duration::from_secs(1), sub.recv())
            .await
            .expect("requeue timed out")
            .unwrap();
        assert_eq!(first.id, second.id);
        sub.ack(second.id).unwrap();
    }

    #[tokio::test]
    async fn dlq_after_max_attempts() {
        let mesh = EventMesh::new();
        let pat = TopicPattern::new("d.>").unwrap();
        let mut sub = mesh.subscribe_with(
            pat,
            SubscribeOptions::default()
                .group("workers")
                .ack(AckMode::explicit()
                    .with_visibility(Duration::from_millis(80))
                    .with_max_attempts(2)
                    .with_dlq("dlq.d")),
        );
        let mut dlq_sub = mesh.subscribe_str("dlq.d").unwrap();
        let t = Topic::new("d.task").unwrap();
        mesh.emit(&t, json!({"v": 1})).await.unwrap();
        // Drain redeliveries without acking until DLQ fires.
        let _ = sub.recv().await.unwrap();
        // After max_attempts, expect DLQ event.
        let dlq_env = tokio::time::timeout(Duration::from_secs(2), dlq_sub.recv())
            .await
            .expect("DLQ timed out")
            .unwrap();
        assert_eq!(dlq_env.topic, "dlq.d");
        assert_eq!(dlq_env.payload["original_topic"], "d.task");
    }

    #[tokio::test]
    async fn drop_subscription_detaches() {
        let mesh = EventMesh::new();
        {
            let _sub = mesh.subscribe_str("x.>").unwrap();
            assert_eq!(mesh.subscriber_count(), 1);
        }
        assert_eq!(mesh.subscriber_count(), 0);
    }
}
