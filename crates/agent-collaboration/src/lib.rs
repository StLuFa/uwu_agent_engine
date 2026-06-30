//! # agent-collaboration
//!
//! 多 Agent 协作 —— 委派 / 协商 + AgentRegistry + CRDT 共享状态。

mod delegate;
mod negotiate;
mod registry;

pub use delegate::{DelegationId, DelegationResult, DelegationState};
pub use negotiate::NegotiationResult;
pub use registry::{AgentDescriptor, AgentRegistry};

use agent_crdt::{GCounter, LWWRegister, ORSet, PNCounter, VectorClock};
use agent_wiki::WikiPage;
use std::collections::HashMap;
use std::sync::Arc;

/// CRDT-backed shared state across collaborating agents.
///
/// Tracks distributed counters, capability sets, and version clocks.
#[derive(Debug, Clone)]
pub struct SharedState {
    /// Per-agent task completion counts (grow-only).
    pub task_counts: HashMap<String, GCounter>,
    /// Active delegation count (PNCounter: inc on delegate, dec on complete).
    pub active_delegations: PNCounter,
    /// Shared capability registry (add-wins set).
    pub capabilities: ORSet<String>,
    /// Per-agent trust scores (LWW register, higher clock wins).
    pub trust_scores: HashMap<String, LWWRegister<f32>>,
    /// Causal history — merged vector clock across all peers.
    pub clock: VectorClock,
}

impl SharedState {
    pub fn new(node_id: impl Into<String>) -> Self {
        let mut clock = VectorClock::new();
        clock.increment(&node_id.into());
        Self {
            task_counts: HashMap::new(),
            active_delegations: PNCounter::new(),
            capabilities: ORSet::new(),
            trust_scores: HashMap::new(),
            clock,
        }
    }

    /// Merge another peer's shared state into ours (CRDT merge).
    pub fn merge(&mut self, other: &SharedState) {
        for (agent, counter) in &other.task_counts {
            self.task_counts
                .entry(agent.clone())
                .and_modify(|c| *c = c.merge(counter))
                .or_insert_with(|| counter.clone());
        }
        self.active_delegations = self.active_delegations.merge(&other.active_delegations);
        self.capabilities = self.capabilities.merge(&other.capabilities);
        for (agent, reg) in &other.trust_scores {
            self.trust_scores
                .entry(agent.clone())
                .and_modify(|r| *r = r.merge(reg))
                .or_insert_with(|| reg.clone());
        }
        self.clock = self.clock.merge(&other.clock);
    }
}

impl Default for SharedState {
    fn default() -> Self { Self::new("default") }
}

/// 协作门面
pub struct Collaboration {
    pub registry: Arc<AgentRegistry>,
    pub pending_delegations: Vec<DelegationResult>,
    /// CRDT shared state with peer agents.
    pub shared_state: SharedState,
    node_id: String,
}

impl Collaboration {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            pending_delegations: Vec::new(),
            shared_state: SharedState::new("agent"),
            node_id: "agent".into(),
        }
    }

    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = node_id.into();
        self.shared_state = SharedState::new(self.node_id.clone());
        self
    }

    /// 根据能力委派任务。
    /// Tracks the delegation in the PNCounter and target agent's task_count.
    pub fn delegate(
        &mut self,
        task_id: impl Into<String>,
        from: agent_types_core::AgentId,
        capability: &str,
    ) -> Option<DelegationResult> {
        let best = self.registry.best_for_capability(capability)?;
        let result = DelegationResult::new(task_id, from, best.agent_id.clone());
        self.pending_delegations.push(result.clone());

        // CRDT: increment active delegations and target's task count
        self.shared_state.active_delegations.inc(&self.node_id, 1);
        let target_id = best.agent_id.to_string();
        self.shared_state
            .task_counts
            .entry(target_id)
            .or_insert_with(GCounter::new)
            .inc(&self.node_id, 1);

        Some(result)
    }

    /// 处理委派完成。Decrements the PNCounter.
    pub fn on_delegation_complete(
        &mut self,
        delegation_id: &DelegationId,
        output: impl Into<String>,
    ) -> Option<DelegationResult> {
        if let Some(idx) = self
            .pending_delegations
            .iter()
            .position(|d| &d.delegation_id == delegation_id)
        {
            let completed = self.pending_delegations[idx].clone().complete(output);
            self.pending_delegations[idx] = completed.clone();

            // CRDT: decrement active delegations
            self.shared_state.active_delegations.dec(&self.node_id, 1);

            Some(completed)
        } else {
            None
        }
    }

    /// 待处理委派数（from CRDT counter — more reliable than Vec count）
    pub fn pending_count(&self) -> usize {
        self.pending_delegations.iter().filter(|d| !d.is_done()).count()
    }

    /// CRDT-reported active delegation count.
    pub fn crdt_active_count(&self) -> i64 {
        self.shared_state.active_delegations.value()
    }

    /// Register a capability in the shared ORSet (add-wins).
    pub fn register_capability(&mut self, capability: String, tag: impl Into<String>) {
        self.shared_state.capabilities.add(capability, tag);
    }

    /// Remove a capability from the shared ORSet.
    pub fn remove_capability(&mut self, capability: &str) {
        self.shared_state.capabilities.remove(&capability.to_string());
    }

    /// Update trust score for a peer agent (LWW register).
    pub fn update_trust(&mut self, agent_id: &str, trust: f32, clock: u64) {
        if let Some(existing) = self.shared_state.trust_scores.get_mut(agent_id) {
            existing.set(trust, clock);
        } else {
            self.shared_state.trust_scores.insert(
                agent_id.to_string(),
                LWWRegister::new(trust, clock, &self.node_id),
            );
        }
    }

    /// Get CRDT-merged trust score for a peer.
    pub fn trust_for(&self, agent_id: &str) -> f32 {
        self.shared_state
            .trust_scores
            .get(agent_id)
            .map(|r| r.value)
            .unwrap_or(0.5)
    }

    /// Merge peer state into local CRDT state.
    pub fn merge_peer_state(&mut self, peer_state: &SharedState) {
        self.shared_state.merge(peer_state);
    }

    /// Delegate a wiki page edit to the best-suited peer agent.
    ///
    /// Creates a delegation for editing the wiki page, returns the delegation result.
    pub fn delegate_wiki_edit(
        &mut self,
        page: &WikiPage,
        editor_agent_id: agent_types_core::AgentId,
    ) -> Option<DelegationResult> {
        let capability = format!("wiki_edit_{}", page.category);
        self.delegate(
            format!("wiki_edit_{}", page.page_id),
            editor_agent_id,
            &capability,
        )
    }

    /// Delegate wiki page creation to a peer agent.
    pub fn delegate_wiki_create(
        &mut self,
        title: &str,
        _content: &str,
        category: &str,
        from: agent_types_core::AgentId,
    ) -> Option<DelegationResult> {
        let task_desc = format!("wiki_create:{title}:{category}");
        let capability = format!("wiki_create_{category}");
        // Register this task type as a capability in the shared ORSet.
        self.shared_state.capabilities.add(
            capability.clone(),
            format!("cap_{}", uuid::Uuid::new_v4()),
        );
        self.delegate(task_desc, from, &capability)
    }
}

// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use agent_types_core::AgentId;

    #[test]
    fn delegate_to_best_agent() {
        let mut registry = AgentRegistry::new();
        let agent_a = AgentDescriptor::new(AgentId::new(), "a", "w")
            .with_capabilities(vec!["search".into()])
            .with_trust(0.8);
        registry.register(agent_a);

        let mut collab = Collaboration::new(Arc::new(registry));
        let from = AgentId::new();
        let result = collab.delegate("task-1", from, "search").unwrap();
        assert_eq!(result.state, DelegationState::Pending);
    }

    #[test]
    fn complete_delegation() {
        let mut registry = AgentRegistry::new();
        let agent_a = AgentDescriptor::new(AgentId::new(), "a", "w")
            .with_capabilities(vec!["click".into()]);
        registry.register(agent_a);

        let mut collab = Collaboration::new(Arc::new(registry));
        let from = AgentId::new();
        let result = collab.delegate("task-2", from, "click").unwrap();
        let did = result.delegation_id;

        let completed = collab.on_delegation_complete(&did, "done").unwrap();
        assert!(completed.is_done());
        assert_eq!(collab.pending_count(), 0);
    }

    // ---- CRDT integration tests ----

    #[test]
    fn crdt_active_count_tracks_delegations() {
        let mut registry = AgentRegistry::new();
        registry.register(
            AgentDescriptor::new(AgentId::new(), "worker", "w")
                .with_capabilities(vec!["task".into()]),
        );
        let mut collab = Collaboration::new(Arc::new(registry)).with_node_id("test-node");
        let from = AgentId::new();

        // 3 delegations
        for i in 0..3 {
            collab.delegate(format!("t-{i}"), from.clone(), "task").unwrap();
        }
        assert_eq!(collab.crdt_active_count(), 3);
    }

    #[test]
    fn crdt_shared_state_merge() {
        let mut s1 = SharedState::new("node-A");
        s1.capabilities.add("search".into(), "tag-1");
        s1.active_delegations.inc("node-A", 3);

        let mut s2 = SharedState::new("node-B");
        s2.capabilities.add("code".into(), "tag-2");
        s2.active_delegations.inc("node-B", 2);

        s1.merge(&s2);
        assert!(s1.capabilities.contains(&"search".to_string()));
        assert!(s1.capabilities.contains(&"code".to_string()));
        assert_eq!(s1.active_delegations.value(), 5); // 3 + 2
    }

    #[test]
    fn crdt_trust_score_update() {
        let mut collab = Collaboration::new(Arc::new(AgentRegistry::new()))
            .with_node_id("node-A");
        collab.update_trust("peer-X", 0.9, 1);
        assert!((collab.trust_for("peer-X") - 0.9).abs() < 0.01);

        // Lower clock → should NOT overwrite
        collab.update_trust("peer-X", 0.1, 0);
        assert!((collab.trust_for("peer-X") - 0.9).abs() < 0.01);

        // Higher clock → should overwrite
        collab.update_trust("peer-X", 0.3, 2);
        assert!((collab.trust_for("peer-X") - 0.3).abs() < 0.01);
    }

    #[test]
    fn crdt_capability_add_remove() {
        let mut collab = Collaboration::new(Arc::new(AgentRegistry::new()));
        let caps = &mut collab.shared_state.capabilities;
        caps.add("search".into(), "t1");
        caps.add("code".into(), "t2");
        assert_eq!(caps.len(), 2);
        caps.remove(&"search".to_string());
        assert!(!caps.contains(&"search".to_string()));
        assert!(caps.contains(&"code".to_string()));
    }

    #[test]
    fn merge_peer_state_updates_collaboration() {
        let mut collab = Collaboration::new(Arc::new(AgentRegistry::new()))
            .with_node_id("main");
        collab.register_capability("search".into(), "t-main");

        let mut peer_state = SharedState::new("peer");
        peer_state.capabilities.add("code".into(), "t-peer");
        peer_state.active_delegations.inc("peer", 5);

        collab.merge_peer_state(&peer_state);
        assert!(collab.shared_state.capabilities.contains(&"search".to_string()));
        assert!(collab.shared_state.capabilities.contains(&"code".to_string()));
        assert_eq!(collab.crdt_active_count(), 5);
    }
}
