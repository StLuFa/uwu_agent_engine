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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Action tests ----

    #[test]
    fn action_new_generates_unique_id() {
        let a1 = Action::new("click", ActionParams::new());
        let a2 = Action::new("click", ActionParams::new());
        assert_ne!(a1.id, a2.id, "each action must have a unique id");
    }

    #[test]
    fn action_new_sets_command_and_timestamp() {
        let action = Action::new("scroll", ActionParams::new().with("dx", 100));
        assert_eq!(action.command, "scroll");
        assert!(action.timestamp <= chrono::Utc::now());
    }

    #[test]
    fn action_serde_roundtrip() {
        let action = Action::new("type", ActionParams::new().with("key", "value"));
        let json = serde_json::to_string(&action).unwrap();
        let decoded: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, action.id);
        assert_eq!(decoded.command, action.command);
        assert_eq!(decoded.params.get("key").unwrap().as_str().unwrap(), "value");
    }

    // ---- ActionParams tests ----

    #[test]
    fn action_params_new_is_empty() {
        let p = ActionParams::new();
        assert!(p.is_empty());
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn action_params_with_chains() {
        let p = ActionParams::new()
            .with("x", 10)
            .with("y", "hello")
            .with("z", true);
        assert_eq!(p.len(), 3);
        assert!(!p.is_empty());
    }

    #[test]
    fn action_params_get_returns_correct_type() {
        let p = ActionParams::new()
            .with("int", 42u64)
            .with("str", "text")
            .with("bool", true);
        assert_eq!(p.get("int").unwrap().as_u64().unwrap(), 42);
        assert_eq!(p.get("str").unwrap().as_str().unwrap(), "text");
        assert_eq!(p.get("bool").unwrap().as_bool().unwrap(), true);
    }

    #[test]
    fn action_params_get_missing_returns_none() {
        let p = ActionParams::new().with("key", 1);
        assert!(p.get("nonexistent").is_none());
    }

    #[test]
    fn action_params_serde_roundtrip() {
        let p = ActionParams::new()
            .with("a", 1)
            .with("b", "two");
        let json = serde_json::to_string(&p).unwrap();
        let decoded: ActionParams = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded.get("a").unwrap().as_u64().unwrap(), 1);
        assert_eq!(decoded.get("b").unwrap().as_str().unwrap(), "two");
    }

    #[test]
    fn action_params_default_is_empty() {
        let p = ActionParams::default();
        assert!(p.is_empty());
    }

    // ---- ActionStatus tests ----

    #[test]
    fn action_status_equality() {
        assert_eq!(ActionStatus::Hypothetical, ActionStatus::Hypothetical);
        assert_eq!(ActionStatus::Committed, ActionStatus::Committed);
        assert_eq!(ActionStatus::Reverted, ActionStatus::Reverted);
        assert_ne!(ActionStatus::Hypothetical, ActionStatus::Committed);
        assert_ne!(ActionStatus::Committed, ActionStatus::Reverted);
    }

    #[test]
    fn action_status_serde_roundtrip() {
        for status in [ActionStatus::Hypothetical, ActionStatus::Committed, ActionStatus::Reverted] {
            let json = serde_json::to_string(&status).unwrap();
            let decoded: ActionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, status);
        }
    }
}
