//! Subscription-side types: backpressure policy, options, sender/receiver,
//! filter, group, ack mode.

use std::sync::Arc;
use std::sync::Weak;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::core::envelope::Envelope;
use crate::core::error::{EventMeshError, Result};
use crate::core::topic::TopicPattern;
use uuid::Uuid;

use crate::ext::filter::Filter;
use super::ring::{self, RingReceiver, RingSender};

/// Unique broker-assigned id for a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubId(pub(super) u64);

/// What to do when a subscriber's buffer is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressurePolicy {
    /// Await capacity (publisher is throttled by the slowest consumer).
    Block,
    /// Drop the incoming envelope for this subscriber.
    DropNewest,
    /// Drop the oldest queued envelope to make room for the new one.
    DropOldest,
    /// Disconnect the subscriber on first overflow.
    Disconnect,
}

impl Default for BackpressurePolicy {
    fn default() -> Self {
        Self::Block
    }
}

/// Group dispatch strategy.
#[derive(Debug, Clone)]
pub enum GroupStrategy {
    /// Round-robin across active members.
    RoundRobin,
    /// Hash on `headers[key]` to pick a member; same key always lands on
    /// the same member while membership is stable.
    KeyHash { header_key: String },
}

impl Default for GroupStrategy {
    fn default() -> Self {
        Self::RoundRobin
    }
}

/// Acknowledgement mode for a subscription.
#[derive(Debug, Clone)]
pub enum AckMode {
    /// Fire-and-forget. Once delivered, the broker forgets about the event.
    Auto,
    /// Consumer must call [`Subscription::ack`] within `visibility`.
    /// Otherwise the broker re-delivers, up to `max_attempts`. After the
    /// final attempt fails, the envelope is forwarded to `dlq_topic` if
    /// set, otherwise dropped.
    Explicit {
        visibility: Duration,
        max_attempts: u32,
        dlq_topic: Option<String>,
    },
}

impl Default for AckMode {
    fn default() -> Self {
        Self::Auto
    }
}

impl AckMode {
    pub fn explicit() -> Self {
        Self::Explicit {
            visibility: Duration::from_secs(30),
            max_attempts: 5,
            dlq_topic: None,
        }
    }
    pub fn with_visibility(mut self, d: Duration) -> Self {
        if let Self::Explicit { visibility, .. } = &mut self {
            *visibility = d;
        }
        self
    }
    pub fn with_max_attempts(mut self, n: u32) -> Self {
        if let Self::Explicit { max_attempts, .. } = &mut self {
            *max_attempts = n;
        }
        self
    }
    pub fn with_dlq(mut self, topic: impl Into<String>) -> Self {
        if let Self::Explicit { dlq_topic, .. } = &mut self {
            *dlq_topic = Some(topic.into());
        }
        self
    }
}

#[derive(Clone, Default)]
pub struct SubscribeOptions {
    pub buffer: usize,
    pub policy: BackpressurePolicy,
    pub filter: Option<Filter>,
    pub group: Option<String>,
    pub group_strategy: GroupStrategy,
    pub ack: AckMode,
}

impl std::fmt::Debug for SubscribeOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscribeOptions")
            .field("buffer", &self.buffer)
            .field("policy", &self.policy)
            .field("filter", &self.filter.is_some())
            .field("group", &self.group)
            .field("group_strategy", &self.group_strategy)
            .field("ack", &self.ack)
            .finish()
    }
}

impl SubscribeOptions {
    pub fn buffer(mut self, n: usize) -> Self {
        self.buffer = n;
        self
    }
    pub fn policy(mut self, p: BackpressurePolicy) -> Self {
        self.policy = p;
        self
    }
    pub fn filter(mut self, f: Filter) -> Self {
        self.filter = Some(f);
        self
    }
    pub fn group(mut self, name: impl Into<String>) -> Self {
        self.group = Some(name.into());
        self
    }
    pub fn group_strategy(mut self, s: GroupStrategy) -> Self {
        self.group_strategy = s;
        self
    }
    pub fn ack(mut self, mode: AckMode) -> Self {
        self.ack = mode;
        self
    }

    pub(super) fn with_default_buffer(mut self) -> Self {
        if self.buffer == 0 {
            self.buffer = super::broker::DEFAULT_BUFFER;
        }
        self
    }
}

// ---- internal sender/receiver split ----------------------------------------

pub(super) enum SubSender {
    Bounded(mpsc::Sender<Arc<Envelope>>),
    Ring(RingSender<Arc<Envelope>>),
}

impl SubSender {
    pub fn is_closed(&self) -> bool {
        match self {
            SubSender::Bounded(s) => s.is_closed(),
            SubSender::Ring(_) => false,
        }
    }
}

#[derive(Clone)]
pub(super) enum SenderHandle {
    Bounded(mpsc::Sender<Arc<Envelope>>),
    Ring(RingSender<Arc<Envelope>>),
}

impl From<&SubSender> for SenderHandle {
    fn from(s: &SubSender) -> Self {
        match s {
            SubSender::Bounded(t) => SenderHandle::Bounded(t.clone()),
            SubSender::Ring(r) => SenderHandle::Ring(r.clone()),
        }
    }
}

pub(super) enum SubReceiver {
    Bounded(mpsc::Receiver<Arc<Envelope>>),
    Ring(RingReceiver<Arc<Envelope>>),
}

/// Receiver handle for a subscription.
pub struct Subscription {
    pub(super) rx: SubReceiver,
    pub(super) pattern: TopicPattern,
    pub(super) policy: BackpressurePolicy,
    pub(super) id: SubId,
    pub(super) inner: Weak<super::broker::Inner>,
    pub(super) group: Option<String>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            super::broker::detach_subscriber(&inner, self.id, self.group.as_deref());
        }
    }
}

/// Error returned by `Subscription::try_recv`.
#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

impl From<mpsc::error::TryRecvError> for TryRecvError {
    fn from(e: mpsc::error::TryRecvError) -> Self {
        match e {
            mpsc::error::TryRecvError::Empty => TryRecvError::Empty,
            mpsc::error::TryRecvError::Disconnected => TryRecvError::Disconnected,
        }
    }
}

impl From<ring::TryRecvError> for TryRecvError {
    fn from(e: ring::TryRecvError) -> Self {
        match e {
            ring::TryRecvError::Empty => TryRecvError::Empty,
            ring::TryRecvError::Disconnected => TryRecvError::Disconnected,
        }
    }
}

/// How to handle a `nack`.
#[derive(Debug, Clone)]
pub enum Requeue {
    /// Re-deliver immediately (counts as one attempt).
    Immediate,
    /// Schedule re-delivery after `delay`.
    Delay(Duration),
    /// Send straight to DLQ (or drop if no DLQ configured).
    Dead,
}

impl Subscription {
    pub async fn recv(&mut self) -> Option<Arc<Envelope>> {
        match &mut self.rx {
            SubReceiver::Bounded(r) => r.recv().await,
            SubReceiver::Ring(r) => r.recv().await,
        }
    }

    pub fn try_recv(&mut self) -> std::result::Result<Arc<Envelope>, TryRecvError> {
        match &mut self.rx {
            SubReceiver::Bounded(r) => r.try_recv().map_err(Into::into),
            SubReceiver::Ring(r) => r.try_recv().map_err(Into::into),
        }
    }

    /// Pull-style batch consume. Awaits up to `timeout` for the first
    /// envelope; once at least one arrives, drains up to `max` more
    /// without blocking and returns.
    pub async fn poll(&mut self, max: usize, timeout: Duration) -> Vec<Arc<Envelope>> {
        if max == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(max);
        let first = match tokio::time::timeout(timeout, self.recv()).await {
            Ok(Some(env)) => env,
            _ => return out,
        };
        out.push(first);
        while out.len() < max {
            match self.try_recv() {
                Ok(env) => out.push(env),
                Err(_) => break,
            }
        }
        out
    }

    /// Acknowledge an envelope (only meaningful in `AckMode::Explicit`).
    pub fn ack(&self, env_id: Uuid) -> Result<()> {
        let inner = self.inner.upgrade().ok_or(EventMeshError::Closed)?;
        super::broker::ack_envelope(&inner, self.id, env_id);
        Ok(())
    }

    /// Negative acknowledge: trigger requeue / DLQ flow.
    pub fn nack(&self, env_id: Uuid, mode: Requeue) -> Result<()> {
        let inner = self.inner.upgrade().ok_or(EventMeshError::Closed)?;
        super::broker::nack_envelope(&inner, self.id, env_id, mode);
        Ok(())
    }

    pub fn pattern(&self) -> &TopicPattern {
        &self.pattern
    }
    pub fn policy(&self) -> BackpressurePolicy {
        self.policy
    }
    pub fn id(&self) -> SubId {
        self.id
    }
    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
}

/// Internal subscriber record stored in the broker.
pub(super) struct Subscriber {
    pub id: SubId,
    pub pattern: TopicPattern,
    pub sender: SubSender,
    pub policy: BackpressurePolicy,
    pub filter: Option<Filter>,
    pub group: Option<String>,
    pub ack: AckMode,
}
