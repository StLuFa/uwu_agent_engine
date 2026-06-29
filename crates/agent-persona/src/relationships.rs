//! RelationshipGraph

use agent_types_core::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 关系类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationType {
    /// 同级协作者
    Peer,
    /// 上级（可委派任务）
    Supervisor,
    /// 下级（可接收委派）
    Subordinate,
    /// 外部实体
    External,
}

/// 与另一个 Agent 的关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// 关系类型
    pub relation_type: RelationType,
    /// 信任度 [0.0, 1.0]
    pub trust: f32,
    /// 协作次数
    pub collaboration_count: u32,
    /// 最近交互描述
    pub last_interaction: Option<String>,
}

impl Relationship {
    pub fn new(relation_type: RelationType, trust: f32) -> Self {
        Self {
            relation_type,
            trust: trust.clamp(0.0, 1.0),
            collaboration_count: 0,
            last_interaction: None,
        }
    }
}

/// Agent 关系图 —— AgentId → Relationship 的映射
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationshipGraph {
    edges: HashMap<AgentId, Relationship>,
}

impl RelationshipGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// 添加或更新关系
    pub fn upsert(&mut self, agent: AgentId, relationship: Relationship) {
        self.edges.insert(agent, relationship);
    }

    /// 调整信任度（delta 范围为 [-1.0, 1.0]）
    pub fn adjust_trust(&mut self, agent: AgentId, trust_delta: f32) {
        let entry = self.edges.entry(agent).or_insert_with(|| {
            Relationship::new(RelationType::Peer, 0.5)
        });
        entry.trust = (entry.trust + trust_delta).clamp(0.0, 1.0);
        entry.collaboration_count += 1;
    }

    /// 获取对某 Agent 的信任度
    pub fn trust_for(&self, agent: &AgentId) -> f32 {
        self.edges.get(agent).map(|r| r.trust).unwrap_or(0.0)
    }

    /// 获取信任度最高的 peers 列表（信任度 > 0.5）
    pub fn trusted_peers(&self) -> Vec<(AgentId, f32)> {
        let mut peers: Vec<_> = self
            .edges
            .iter()
            .filter(|(_, r)| r.trust > 0.5)
            .map(|(id, r)| (id.clone(), r.trust))
            .collect();
        peers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        peers
    }

    /// 关系数量
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// 遍历所有关系
    pub fn iter(&self) -> impl Iterator<Item = (&AgentId, &Relationship)> {
        self.edges.iter()
    }
}
