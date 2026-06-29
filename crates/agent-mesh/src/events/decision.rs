//! Decision 事件

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 决策完成事件
///
/// Topic: `"agent.decision.made"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionMade {
    pub event_id: String,
    pub agent_id: String,
    pub decision_text: String,
    pub meta_score: f32,
    pub meta_action: String,
    pub tokens_used: u64,
    pub timestamp: DateTime<Utc>,
}

impl DecisionMade {
    pub fn new(
        agent_id: impl Into<String>,
        decision_text: impl Into<String>,
        meta_score: f32,
        meta_action: impl Into<String>,
        tokens_used: u64,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            decision_text: decision_text.into(),
            meta_score,
            meta_action: meta_action.into(),
            tokens_used,
            timestamp: Utc::now(),
        }
    }
}

/// 决策重试事件
///
/// Topic: `"agent.decision.retried"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRetried {
    pub event_id: String,
    pub agent_id: String,
    pub original_decision_id: String,
    pub reason: String,
    pub retry_count: u32,
    pub timestamp: DateTime<Utc>,
}

impl DecisionRetried {
    pub fn new(
        agent_id: impl Into<String>,
        original_decision_id: impl Into<String>,
        reason: impl Into<String>,
        retry_count: u32,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            original_decision_id: original_decision_id.into(),
            reason: reason.into(),
            retry_count,
            timestamp: Utc::now(),
        }
    }
}
