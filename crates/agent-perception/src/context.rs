//! ContextDescriptor + ParsedInput

use serde::{Deserialize, Serialize};

// Re-export ContextDescriptor from agent-state（避免重复定义）
pub use agent_state::short::ContextDescriptor;

/// 解析后的结构化输入 —— PII 扫描的前置步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedInput {
    /// 原始文本
    pub raw: String,
    /// 提取的纯文本内容
    pub text: String,
    /// 结构化的键值对（从 JSON / 表单中提取）
    pub fields: Vec<(String, String)>,
    /// 是否为结构化输入（JSON / 表单）
    pub is_structured: bool,
}

impl ParsedInput {
    pub fn from_text(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        Self {
            text: raw.clone(),
            raw,
            fields: Vec::new(),
            is_structured: false,
        }
    }

    pub fn from_json(raw: impl Into<String>, fields: Vec<(String, String)>) -> Self {
        let raw = raw.into();
        Self {
            text: raw.clone(),
            raw,
            fields,
            is_structured: true,
        }
    }
}
