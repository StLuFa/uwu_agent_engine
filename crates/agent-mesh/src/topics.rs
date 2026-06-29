//! Agent 领域 topic 命名空间常量

/// 状态事件: `"agent.state.>"`
pub const TOPIC_STATE: &str = "agent.state.>";

/// 任务事件: `"agent.task.>"`
pub const TOPIC_TASK: &str = "agent.task.>";

/// 决策事件: `"agent.decision.>"`
pub const TOPIC_DECISION: &str = "agent.decision.>";

/// Persona 事件: `"agent.persona.>"`
pub const TOPIC_PERSONA: &str = "agent.persona.>";

/// 具体 topic 名
pub const TOPIC_STATE_SNAPSHOT: &str = "agent.state.snapshot";
pub const TOPIC_TASK_CREATED: &str = "agent.task.created";
pub const TOPIC_TASK_COMPLETED: &str = "agent.task.completed";
pub const TOPIC_SUBTASK_DELEGATED: &str = "agent.task.subtask_delegated";
pub const TOPIC_DELEGATION_RESULT: &str = "agent.task.delegation_result";
pub const TOPIC_DECISION_MADE: &str = "agent.decision.made";
pub const TOPIC_DECISION_RETRIED: &str = "agent.decision.retried";
pub const TOPIC_PERSONA_UPDATED: &str = "agent.persona.updated";
pub const TOPIC_RELATIONSHIP_CHANGED: &str = "agent.persona.relationship_changed";
