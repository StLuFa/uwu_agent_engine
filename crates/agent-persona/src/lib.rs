//! # agent-persona
//!
//! 人物角色（MVCC 版本化）—— Agent 的"我是谁"。
//!
//! 随经历增长而变化：每一次协作都在更新关系图。
//! Persona 与 Character 的区别：
//! - **Persona**（可变）：身份/关系/履历 → "我是什么样的人（会变）"
//! - **Character**（不可变）：核心价值观/偏好 → "我是什么样的人（不变）"
//!
//! ## MVCC
//!
//! - 主进程写入：`update_relationship()` → version += 1
//! - Sidecar 读取：`snapshot()` → 只读快照

mod history;
mod identity;
mod relationships;

pub use history::{PersonaEvent, PersonaHistory};
pub use identity::Identity;
pub use relationships::{RelationType, Relationship, RelationshipGraph};

use agent_types_core::AgentId;
use serde::{Deserialize, Serialize};

/// 人物角色 —— Agent 的身份、关系网络和履历
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// MVCC 版本号
    pub version: u64,
    /// 身份
    pub identity: Identity,
    /// 关系图
    pub relationships: RelationshipGraph,
    /// 关键经历
    pub history: PersonaHistory,
}

impl Persona {
    /// 生成上下文注入字符串（供推理时注入 prompt）
    pub fn to_context_injection(&self) -> PersonaContext {
        PersonaContext {
            name: self.identity.name.clone(),
            role: self.identity.role.clone(),
            expertise: self.identity.expertise.clone(),
            trust_peers: self.relationships.trusted_peers(),
        }
    }

    /// 根据协作结果更新关系
    pub fn update_relationship(&mut self, peer: AgentId, trust_delta: f32) {
        self.version += 1;
        self.relationships.adjust_trust(peer, trust_delta);
    }

    /// 生成只读快照（供 Sidecar 消费）
    pub fn snapshot(&self) -> PersonaSnapshot {
        PersonaSnapshot {
            version: self.version,
            identity: self.identity.clone(),
            relationship_count: self.relationships.len(),
        }
    }
}

/// PersonaContext —— 注入推理上下文的精简表示
#[derive(Debug, Clone)]
pub struct PersonaContext {
    pub name: String,
    pub role: String,
    pub expertise: Vec<String>,
    pub trust_peers: Vec<(AgentId, f32)>,
}

/// PersonaSnapshot —— 供 Sidecar 只读消费
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSnapshot {
    pub version: u64,
    pub identity: Identity,
    pub relationship_count: usize,
}
