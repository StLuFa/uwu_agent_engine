//! AgentCard / AgentEndpoint / TaskRole

use serde::{Deserialize, Serialize};

/// 任务角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskRole {
    Worker,
    Reviewer,
    Approver,
    Observer,
}

/// Agent 端点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEndpoint {
    pub url: String,
    pub protocol: String,
}

/// Agent 能力名片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub agent_id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub role: TaskRole,
    pub priority: u8,
    pub endpoint: Option<AgentEndpoint>,
}

impl AgentCard {
    pub fn new(agent_id: impl Into<String>, name: impl Into<String>, role: TaskRole) -> Self {
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            capabilities: Vec::new(),
            role,
            priority: 1,
            endpoint: None,
        }
    }
}
