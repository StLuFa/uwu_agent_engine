//! DelegationId / DelegationState / DelegationResult

use agent_types_core::AgentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 委派 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DelegationId(pub String);

impl DelegationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// 委派状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationState {
    Pending,
    Accepted,
    Running,
    Completed,
    Failed { reason: String },
    TimedOut,
}

/// 委派结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResult {
    pub delegation_id: DelegationId,
    pub task_id: String,
    pub from: AgentId,
    pub to: AgentId,
    pub state: DelegationState,
    pub output: Option<String>,
    pub tokens_used: u64,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl DelegationResult {
    pub fn new(task_id: impl Into<String>, from: AgentId, to: AgentId) -> Self {
        Self {
            delegation_id: DelegationId::new(),
            task_id: task_id.into(),
            from,
            to,
            state: DelegationState::Pending,
            output: None,
            tokens_used: 0,
            created_at: Utc::now(),
            completed_at: None,
        }
    }

    pub fn complete(mut self, output: impl Into<String>) -> Self {
        self.state = DelegationState::Completed;
        self.output = Some(output.into());
        self.completed_at = Some(Utc::now());
        self
    }

    pub fn fail(mut self, reason: impl Into<String>) -> Self {
        self.state = DelegationState::Failed {
            reason: reason.into(),
        };
        self.completed_at = Some(Utc::now());
        self
    }

    pub fn is_done(&self) -> bool {
        matches!(
            self.state,
            DelegationState::Completed | DelegationState::Failed { .. } | DelegationState::TimedOut
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegation_lifecycle() {
        let from = AgentId::new();
        let to = AgentId::new();
        let result = DelegationResult::new("task-1", from.clone(), to.clone())
            .complete("done");
        assert!(result.is_done());
        assert!(result.output.is_some());
    }
}
