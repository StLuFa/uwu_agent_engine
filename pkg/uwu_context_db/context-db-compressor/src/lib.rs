//! # agent-context-db-compressor (L6 压缩层)
//!
//! 异步语义处理队列，替代 `agent-sidecar-consolidator`：把 L5 解析层的
//! 重活（L0/L1 生成、去重、轨迹归纳、多模态转文本）从写入热路径剥离到后台 worker。
//!
//! ## 解耦约束
//!
//! - 依赖 session（`SessionHandle`）与 parse（`MemoryCandidate`）的类型，
//!   但只组合它们的**数据形状**，不反向要求它们感知队列。
//! - `SemanticQueue` 是端口（零实现）；具体调度（tokio / NATS）由宿主装配。

use agent_context_db_core::{ContextUri, Result};
use agent_context_db_parse::MemoryCandidate;
use agent_context_db_session::SessionHandle;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// 异步语义处理队列。
#[async_trait]
pub trait SemanticQueue: Send + Sync {
    async fn enqueue(&self, task: SemanticTask) -> Result<TaskId>;
    async fn dequeue(&self) -> Result<Option<(TaskId, SemanticTask)>>;
    /// 标记任务完成，返回结果供订阅方消费。
    async fn complete(&self, id: TaskId, outcome: TaskOutcome) -> Result<()>;
}

/// 语义处理任务（对应 L5 解析层的各个动作）。
#[derive(Debug, Clone)]
pub enum SemanticTask {
    GenerateAbstract(ContextUri),
    GenerateOverview(ContextUri),
    AggregateUpward(ContextUri),
    ExtractMemories {
        archive: ContextUri,
        session: Box<SessionHandle>,
    },
    DeduplicateMemories(Vec<MemoryCandidate>),
    ExtractTrajectory(ContextUri),
    InduceExperience(Vec<ContextUri>),
    MultimodalToText(ContextUri),
}

#[derive(Debug, Clone)]
pub struct TaskDoneEvent {
    pub task_id: TaskId,
    pub outcome: TaskOutcome,
}

#[derive(Debug, Clone)]
pub enum TaskOutcome {
    Success,
    PartialFailure(String),
    Failure(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_variants_compile() {
        let t = SemanticTask::GenerateAbstract(
            ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap(),
        );
        assert!(matches!(t, SemanticTask::GenerateAbstract(_)));
        assert_ne!(TaskId::new(), TaskId::new());
    }
}
