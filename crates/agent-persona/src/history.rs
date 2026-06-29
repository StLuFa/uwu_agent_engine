//! PersonaHistory

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 一次关键经历事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaEvent {
    /// 事件描述
    pub description: String,
    /// 事件类型标签
    pub event_type: String,
    /// 发生时间
    pub occurred_at: DateTime<Utc>,
    /// 涉及的 Agent（如有）
    pub involved_agents: Vec<String>,
}

impl PersonaEvent {
    pub fn new(description: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            event_type: event_type.into(),
            occurred_at: Utc::now(),
            involved_agents: Vec::new(),
        }
    }

    pub fn with_agents(mut self, agents: Vec<String>) -> Self {
        self.involved_agents = agents;
        self
    }
}

/// 关键经历日志 —— 按时间排序的事件序列
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonaHistory {
    events: Vec<PersonaEvent>,
}

impl PersonaHistory {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// 追加事件
    pub fn push(&mut self, event: PersonaEvent) {
        self.events.push(event);
    }

    /// 事件数量
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// 最近 n 条事件（按时间从旧到新）
    pub fn recent(&self, n: usize) -> Vec<&PersonaEvent> {
        self.events.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
    }

    /// 按类型筛选事件
    pub fn by_type(&self, event_type: &str) -> Vec<&PersonaEvent> {
        self.events
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// 所有事件迭代器
    pub fn iter(&self) -> impl Iterator<Item = &PersonaEvent> {
        self.events.iter()
    }
}
