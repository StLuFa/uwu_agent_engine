//! # agent-collaboration
//!
//! 多 Agent 协作 —— 委派 / 协商 / CRDT 状态合并 + AgentRegistry。
//!
//! ## 核心能力
//!
//! - `delegate()` — 根据 DelegationPolicy 选择 Agent → 发送 subtask → 等待结果
//! - `negotiate()` — CRDT 状态协商合并
//! - `AgentRegistry` — 维护已知 Agent 的能力索引

mod delegate;
mod negotiate;
mod registry;

pub use delegate::{DelegationId, DelegationState, DelegationResult};
pub use negotiate::NegotiationResult;
pub use registry::{AgentDescriptor, AgentRegistry};

use agent_mesh::AgentMesh;
use agent_state::AgentState;
use agent_types_core::AgentId;
use std::sync::Arc;

/// 协作门面
pub struct Collaboration {
    pub registry: Arc<AgentRegistry>,
    pub mesh: Arc<AgentMesh>,
}

impl Collaboration {
    pub fn new(registry: Arc<AgentRegistry>, mesh: Arc<AgentMesh>) -> Self {
        Self { registry, mesh }
    }
}
