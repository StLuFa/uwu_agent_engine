//! # agent-collaboration
//!
//! 多 Agent 协作 —— 委派 / 协商 + AgentRegistry。

mod delegate;
mod negotiate;
mod registry;

pub use delegate::{DelegationId, DelegationResult, DelegationState};
pub use negotiate::NegotiationResult;
pub use registry::{AgentDescriptor, AgentRegistry};

use std::sync::Arc;

/// 协作门面
pub struct Collaboration {
    pub registry: Arc<AgentRegistry>,
    pub pending_delegations: Vec<DelegationResult>,
}

impl Collaboration {
    pub fn new(registry: Arc<AgentRegistry>) -> Self {
        Self {
            registry,
            pending_delegations: Vec::new(),
        }
    }

    /// 根据能力委派任务
    pub fn delegate(
        &mut self,
        task_id: impl Into<String>,
        from: agent_types_core::AgentId,
        capability: &str,
    ) -> Option<DelegationResult> {
        let best = self.registry.best_for_capability(capability)?;
        let result = DelegationResult::new(task_id, from, best.agent_id.clone());
        self.pending_delegations.push(result.clone());
        Some(result)
    }

    /// 处理委派完成
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
            Some(completed)
        } else {
            None
        }
    }

    /// 待处理委派数
    pub fn pending_count(&self) -> usize {
        self.pending_delegations.iter().filter(|d| !d.is_done()).count()
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
}
