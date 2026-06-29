//! Identity struct

use serde::{Deserialize, Serialize};

/// Agent 身份 —— "我是谁"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// 名称 / 显示名
    pub name: String,
    /// 角色（如 "assistant", "researcher", "executor"）
    pub role: String,
    /// 所属组织
    pub organization: String,
    /// 背景描述
    pub background: String,
    /// 专长领域
    pub expertise: Vec<String>,
}

impl Identity {
    pub fn new(name: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: role.into(),
            organization: String::new(),
            background: String::new(),
            expertise: Vec::new(),
        }
    }

    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = org.into();
        self
    }

    pub fn with_background(mut self, bg: impl Into<String>) -> Self {
        self.background = bg.into();
        self
    }

    pub fn with_expertise(mut self, expertise: Vec<String>) -> Self {
        self.expertise = expertise;
        self
    }
}

impl Default for Identity {
    fn default() -> Self {
        Self::new("agent", "assistant")
    }
}
