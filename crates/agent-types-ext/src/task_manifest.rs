//! TaskManifest type

use serde::{Deserialize, Serialize};

/// 任务清单
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
