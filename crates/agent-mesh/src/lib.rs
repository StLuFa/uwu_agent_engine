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
