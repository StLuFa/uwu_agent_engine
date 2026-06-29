//! Action / ActionParams / ActionStatus types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent 可执行动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub command: String,
    pub params: ActionParams,
    pub timestamp: DateTime<Utc>,
}

impl Action {
    pub fn new(command: impl Into<String>, params: ActionParams) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            command: command.into(),
            params,
            timestamp: Utc::now(),
        }
    }
}

/// Action 参数 —— 扁平 key-value 结构
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionParams(pub HashMap<String, serde_json::Value>);

impl ActionParams {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.0.get(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Action 生命周期状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionStatus {
    /// 沙盒推演中（未提交）
    Hypothetical,
    /// 已提交到主状态
    Committed,
    /// 已回滚
    Reverted,
}
