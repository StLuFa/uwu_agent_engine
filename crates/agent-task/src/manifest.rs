//! TaskManifest + AgentCard

use serde::{Deserialize, Serialize};

/// 任务清单 —— 描述任务的需求和约束
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskManifest {
    pub title: String,
    pub description: String,
    pub required_capabilities: Vec<String>,
    pub estimated_tokens: u64,
    pub deadline_secs: Option<u64>,
    pub priority: u8,
}

impl Default for TaskManifest {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            required_capabilities: Vec::new(),
            estimated_tokens: 0,
            deadline_secs: None,
            priority: 5,
        }
    }
}

impl TaskManifest {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            ..Default::default()
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.required_capabilities = caps;
        self
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Agent 能力卡片（阶段 5+ 协作委派使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct AgentCard {
    pub agent_id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub role: String,
    pub priority: u8,
    pub endpoint: Option<String>,
    pub trust_score: f32,
}

#[allow(dead_code)]
impl AgentCard {
    pub fn new(agent_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            name: name.into(),
            capabilities: Vec::new(),
            role: "worker".into(),
            priority: 1,
            endpoint: None,
            trust_score: 0.5,
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }
}
