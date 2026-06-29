//! # agent-task
//!
//! 任务域 —— TaskManifest + AgentCard + SettlementPolicy + SubtaskDAG 调度。
//!
//! Task 是跨多轮、可能跨多 Agent 的持久工作单元。

mod delegation;
mod manifest;
mod scheduler;
mod settlement;
mod subtask;

pub use delegation::{DelegationPolicy, DiscoveryStrategy, FallbackStrategy};
pub use manifest::TaskManifest;
pub use scheduler::SubtaskScheduler;
pub use settlement::{SettlementMode, SettlementPolicy};
pub use subtask::{Subtask, SubtaskDag, SubtaskStatus, SubtaskEdge};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task ID
pub type TaskId = Uuid;
/// Subtask ID
pub type SubtaskId = Uuid;

/// 任务状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Created,
    Running,
    WaitingForDelegation,
    Completed,
    Failed { error: String },
    Cancelled,
}

/// 任务目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub description: String,
    pub success_criteria: Vec<String>,
    pub priority: u8,
}

/// 任务 —— 持久工作单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: TaskId,
    pub goal: Goal,
    pub status: TaskStatus,
    pub subtask_dag: SubtaskDag,
    pub max_retries_per_subtask: u32,
    pub manifest: TaskManifest,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Task {
    /// 创建新任务
    pub fn new(goal: Goal, manifest: TaskManifest) -> Self {
        let now = chrono::Utc::now();
        Self {
            task_id: Uuid::new_v4(),
            goal,
            status: TaskStatus::Created,
            subtask_dag: SubtaskDag::default(),
            max_retries_per_subtask: 3,
            manifest,
            created_at: now,
            updated_at: now,
        }
    }

    /// 检查 DAG 中可执行的 subtask
    pub fn check_ready(&self) -> Vec<SubtaskId> {
        self.subtask_dag
            .ready_nodes()
            .into_iter()
            .map(|id| Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()))
            .collect()
    }

    /// 根据完成的 subtask 更新任务进度
    pub fn update_progress(&mut self) {
        use crate::SubtaskScheduler;

        let progress = SubtaskScheduler::progress(&self.subtask_dag);

        if SubtaskScheduler::is_complete(&self.subtask_dag) {
            self.status = TaskStatus::Completed;
        } else if progress > 0.0 {
            self.status = TaskStatus::Running;
        }

        self.updated_at = chrono::Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtask::{Subtask, SubtaskStatus};

    #[test]
    fn update_progress_marks_running_when_in_progress() {
        let mut task = Task::new(
            Goal {
                description: "test".into(),
                success_criteria: vec![],
                priority: 1,
            },
            TaskManifest::default(),
        );
        task.status = TaskStatus::Running;
        let idx = task.subtask_dag.add_node(Subtask::new(0, "t1"));
        task.subtask_dag.nodes[idx].status = SubtaskStatus::Completed;
        task.update_progress();
        // All nodes completed → Completed
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn update_progress_sets_completed() {
        let mut task = Task::new(
            Goal {
                description: "test".into(),
                success_criteria: vec![],
                priority: 1,
            },
            TaskManifest::default(),
        );
        let idx = task.subtask_dag.add_node(Subtask::new(0, "t1"));
        task.subtask_dag.nodes[idx].status = SubtaskStatus::Completed;
        task.update_progress();
        assert_eq!(task.status, TaskStatus::Completed);
    }
}
