//! AgentRegistry + AgentDescriptor

use agent_types_core::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent 能力描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub agent_id: AgentId,
    pub name: String,
    pub capabilities: Vec<String>,
    pub role: String,
    pub trust_score: f32,
    pub is_available: bool,
    pub task_count: u64,
}

impl AgentDescriptor {
    pub fn new(agent_id: AgentId, name: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            agent_id,
            name: name.into(),
            capabilities: Vec::new(),
            role: role.into(),
            trust_score: 0.5,
            is_available: true,
            task_count: 0,
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_trust(mut self, trust: f32) -> Self {
        self.trust_score = trust.clamp(0.0, 1.0);
        self
    }
}

/// Agent 注册表 —— 维护已知 Agent 的能力索引
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentRegistry {
    agents: HashMap<AgentId, AgentDescriptor>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// 注册 Agent
    pub fn register(&mut self, desc: AgentDescriptor) {
        self.agents.insert(desc.agent_id.clone(), desc);
    }

    /// 注销 Agent
    pub fn unregister(&mut self, id: &AgentId) {
        self.agents.remove(id);
    }

    /// 获取 Agent 描述
    pub fn get(&self, id: &AgentId) -> Option<&AgentDescriptor> {
        self.agents.get(id)
    }

    /// 按能力查找 Agent（零分配查找）
    pub fn find_by_capability(&self, capability: &str) -> Vec<&AgentDescriptor> {
        self.agents
            .values()
            .filter(|d| d.is_available && d.capabilities.iter().any(|c| c == capability))
            .collect()
    }

    /// 按信任度排序获取最优 Agent
    pub fn best_for_capability(&self, capability: &str) -> Option<&AgentDescriptor> {
        let mut candidates: Vec<_> = self.find_by_capability(capability);
        candidates.sort_by(|a, b| b.trust_score.total_cmp(&a.trust_score));
        candidates.into_iter().next()
    }

    /// Agent 数量
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_find_by_capability() {
        let mut registry = AgentRegistry::new();
        let desc = AgentDescriptor::new(AgentId::new(), "agent-a", "worker")
            .with_capabilities(vec!["search".into(), "click".into()]);
        registry.register(desc);

        let found = registry.find_by_capability("search");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn best_capability_picks_highest_trust() {
        let mut registry = AgentRegistry::new();
        let a = AgentDescriptor::new(AgentId::new(), "a", "w")
            .with_capabilities(vec!["search".into()])
            .with_trust(0.3);
        let b = AgentDescriptor::new(AgentId::new(), "b", "w")
            .with_capabilities(vec!["search".into()])
            .with_trust(0.9);
        registry.register(a);
        registry.register(b);

        let best = registry.best_for_capability("search").unwrap();
        assert!((best.trust_score - 0.9).abs() < 0.001);
    }
}
