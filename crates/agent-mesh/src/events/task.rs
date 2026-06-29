//! Task 事件

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 任务创建事件
///
/// Topic: `"agent.task.created"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreated {
    pub event_id: String,
    pub task_id: String,
    pub goal_description: String,
    pub priority: u8,
    pub created_by: String,
    pub timestamp: DateTime<Utc>,
}

impl TaskCreated {
    pub fn new(
        task_id: impl Into<String>,
        goal_description: impl Into<String>,
        priority: u8,
        created_by: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            goal_description: goal_description.into(),
            priority,
            created_by: created_by.into(),
            timestamp: Utc::now(),
        }
    }
}

/// 任务完成事件
///
/// Topic: `"agent.task.completed"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompleted {
    pub event_id: String,
    pub task_id: String,
    pub completed_by: String,
    pub success: bool,
    pub summary: String,
    pub timestamp: DateTime<Utc>,
}

impl TaskCompleted {
    pub fn new(
        task_id: impl Into<String>,
        completed_by: impl Into<String>,
        success: bool,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            completed_by: completed_by.into(),
            success,
            summary: summary.into(),
            timestamp: Utc::now(),
        }
    }
}

/// 子任务委派事件
///
/// Topic: `"agent.task.subtask_delegated"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskDelegated {
    pub event_id: String,
    pub task_id: String,
    pub subtask_id: String,
    pub delegated_by: String,
    pub delegated_to: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
}

impl SubtaskDelegated {
    pub fn new(
        task_id: impl Into<String>,
        subtask_id: impl Into<String>,
        delegated_by: impl Into<String>,
        delegated_to: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            subtask_id: subtask_id.into(),
            delegated_by: delegated_by.into(),
            delegated_to: delegated_to.into(),
            description: description.into(),
            timestamp: Utc::now(),
        }
    }
}

/// 委派结果事件
///
/// Topic: `"agent.task.delegation_result"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResult {
    pub event_id: String,
    pub task_id: String,
    pub subtask_id: String,
    pub completed_by: String,
    pub success: bool,
    pub result_summary: String,
    pub timestamp: DateTime<Utc>,
}

impl DelegationResult {
    pub fn new(
        task_id: impl Into<String>,
        subtask_id: impl Into<String>,
        completed_by: impl Into<String>,
        success: bool,
        result_summary: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.into(),
            subtask_id: subtask_id.into(),
            completed_by: completed_by.into(),
            success,
            result_summary: result_summary.into(),
            timestamp: Utc::now(),
        }
    }
}
