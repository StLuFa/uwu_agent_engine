//! # agent-mesh
//!
//! Agent 语义事件网格 —— 对 [`uwu_event_mesh`] 的领域包装。
//!
//! 不重复实现底层事件机制，仅定义 Agent 领域的：
//! - **topic 命名空间**（`agent.state.snapshot` / `agent.task.created` / …）
//! - **事件类型**（StateSnapshotEvent / TaskCreated / DecisionMade / …）
//! - **AgentTypeRegistry**（启动期一次性注册所有 Agent 事件类型）
//! - **AgentMesh 门面**（包装 EventMesh + FlowHandle，提供 Agent 语义的 publish 方法）
//!
//! ## 四路通道语义
//!
//! | 通道 | 容量 | 语义 |
//! |---|---|---|
//! | Main | 64 | 主循环（决策→执行） |
//! | Consolidation | 256 | Sidecar（LearnNode+Guard） |
//! | Monitoring | 64 | Sidecar（异常检测） |
//! | System | 128 | 心跳/配置/关闭 |

pub mod events;
pub mod registry;
pub mod topics;

pub use events::{
    decision::DecisionMade,
    persona::PersonaUpdated,
    state::StateSnapshotEvent,
    task::TaskCreated,
};
pub use registry::AgentTypeRegistry;
pub use topics::{
    TOPIC_DECISION, TOPIC_PERSONA, TOPIC_STATE, TOPIC_TASK,
};

use std::sync::Arc;
use uwu_event_mesh::{EventMesh, FlowHandle, TypeRegistry};

/// Agent 语义的事件网格门面。
///
/// 包装底层 `EventMesh` + `FlowHandle`，所有 publish 方法
/// 自动使用 Agent 领域的 topic 命名空间和类型注册。
pub struct AgentMesh {
    pub mesh: Arc<EventMesh>,
    pub flow: FlowHandle,
    pub type_registry: Arc<TypeRegistry>,
}

impl AgentMesh {
    /// 创建 AgentMesh 并注册所有 Agent 事件类型。
    pub fn new(mesh: Arc<EventMesh>, flow: FlowHandle) -> Self {
        let registry = Arc::new(TypeRegistry::new());
        AgentTypeRegistry::register_all(&registry);
        Self { mesh, flow, type_registry: registry }
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{
        decision::{DecisionMade, DecisionRetried},
        persona::{PersonaUpdated, RelationshipChanged},
        state::StateSnapshotEvent,
        task::{DelegationResult, SubtaskDelegated, TaskCompleted, TaskCreated},
    };

    #[test]
    fn state_snapshot_event_roundtrip() {
        let event = StateSnapshotEvent::new("agent-1", r#"{"version":1}"#, 1);
        let json = serde_json::to_string(&event).unwrap();
        let back: StateSnapshotEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_id, "agent-1");
        assert_eq!(back.snapshot_version, 1);
    }

    #[test]
    fn task_created_event_roundtrip() {
        let event = TaskCreated::new("task-1", "test goal", 5, "agent-1");
        let json = serde_json::to_string(&event).unwrap();
        let back: TaskCreated = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id, "task-1");
        assert_eq!(back.priority, 5);
    }

    #[test]
    fn task_completed_event_roundtrip() {
        let event = TaskCompleted::new("task-1", "agent-2", true, "done");
        let json = serde_json::to_string(&event).unwrap();
        let back: TaskCompleted = serde_json::from_str(&json).unwrap();
        assert!(back.success);
    }

    #[test]
    fn subtask_delegated_event_roundtrip() {
        let event = SubtaskDelegated::new("task-1", "sub-1", "agent-1", "agent-2", "do X");
        let json = serde_json::to_string(&event).unwrap();
        let back: SubtaskDelegated = serde_json::from_str(&json).unwrap();
        assert_eq!(back.delegated_to, "agent-2");
    }

    #[test]
    fn delegation_result_event_roundtrip() {
        let event = DelegationResult::new("task-1", "sub-1", "agent-2", true, "ok");
        let json = serde_json::to_string(&event).unwrap();
        let back: DelegationResult = serde_json::from_str(&json).unwrap();
        assert!(back.success);
    }

    #[test]
    fn decision_made_event_roundtrip() {
        let event = DecisionMade::new("agent-1", "click button", 0.85, "Proceed", 100);
        let json = serde_json::to_string(&event).unwrap();
        let back: DecisionMade = serde_json::from_str(&json).unwrap();
        assert!((back.meta_score - 0.85).abs() < 0.001);
        assert_eq!(back.meta_action, "Proceed");
    }

    #[test]
    fn decision_retried_event_roundtrip() {
        let event = DecisionRetried::new("agent-1", "dec-1", "loop detected", 2);
        let json = serde_json::to_string(&event).unwrap();
        let back: DecisionRetried = serde_json::from_str(&json).unwrap();
        assert_eq!(back.retry_count, 2);
    }

    #[test]
    fn persona_updated_event_roundtrip() {
        let event = PersonaUpdated::new("agent-1", 42, "collaboration completed");
        let json = serde_json::to_string(&event).unwrap();
        let back: PersonaUpdated = serde_json::from_str(&json).unwrap();
        assert_eq!(back.new_version, 42);
    }

    #[test]
    fn relationship_changed_event_roundtrip() {
        let event = RelationshipChanged::new("agent-1", "agent-2", 0.8, 0.2);
        let json = serde_json::to_string(&event).unwrap();
        let back: RelationshipChanged = serde_json::from_str(&json).unwrap();
        assert!((back.new_trust - 0.8).abs() < 0.001);
        assert!((back.trust_delta - 0.2).abs() < 0.001);
    }

    #[test]
    fn topic_constants_defined() {
        assert!(TOPIC_STATE.contains("agent.state"));
        assert!(TOPIC_TASK.contains("agent.task"));
        assert!(TOPIC_DECISION.contains("agent.decision"));
        assert!(TOPIC_PERSONA.contains("agent.persona"));
    }

    #[test]
    fn registry_accepts_all_event_types() {
        let registry = Arc::new(TypeRegistry::new());
        AgentTypeRegistry::register_all(&registry);
        // If this compiles and runs, all types are registered
    }
}
