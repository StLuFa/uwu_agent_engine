//! AgentTypeRegistry —— 启动期一次性注册所有 Agent 事件类型

use crate::events::{
    decision::{DecisionMade, DecisionRetried},
    persona::{PersonaUpdated, RelationshipChanged},
    state::StateSnapshotEvent,
    task::{DelegationResult, SubtaskDelegated, TaskCompleted, TaskCreated},
};
use std::sync::Arc;
use uwu_event_mesh::TypeRegistry;

/// 注册所有 Agent 领域事件类型到 TypeRegistry
pub struct AgentTypeRegistry;

impl AgentTypeRegistry {
    /// 启动期一次性调用：向 TypeRegistry 注册所有 Agent 事件类型。
    ///
    /// 注册后，跨进程反序列化时 TypeRegistry 可校验 payload 类型。
    pub fn register_all(registry: &Arc<TypeRegistry>) {
        registry.register::<StateSnapshotEvent>("agent_state", "snapshot");
        registry.register::<TaskCreated>("agent_task", "created");
        registry.register::<TaskCompleted>("agent_task", "completed");
        registry.register::<SubtaskDelegated>("agent_task", "subtask_delegated");
        registry.register::<DelegationResult>("agent_task", "delegation_result");
        registry.register::<DecisionMade>("agent_decision", "made");
        registry.register::<DecisionRetried>("agent_decision", "retried");
        registry.register::<PersonaUpdated>("agent_persona", "updated");
        registry.register::<RelationshipChanged>("agent_persona", "relationship_changed");
    }
}
