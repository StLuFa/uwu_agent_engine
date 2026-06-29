//! Persona 事件

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Persona 更新事件
///
/// Topic: `"agent.persona.updated"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaUpdated {
    pub event_id: String,
    pub agent_id: String,
    pub new_version: u64,
    pub change_description: String,
    pub timestamp: DateTime<Utc>,
}

impl PersonaUpdated {
    pub fn new(
        agent_id: impl Into<String>,
        new_version: u64,
        change_description: impl Into<String>,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            new_version,
            change_description: change_description.into(),
            timestamp: Utc::now(),
        }
    }
}

/// 关系变更事件
///
/// Topic: `"agent.persona.relationship_changed"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipChanged {
    pub event_id: String,
    pub agent_id: String,
    pub peer_id: String,
    pub new_trust: f32,
    pub trust_delta: f32,
    pub timestamp: DateTime<Utc>,
}

impl RelationshipChanged {
    pub fn new(
        agent_id: impl Into<String>,
        peer_id: impl Into<String>,
        new_trust: f32,
        trust_delta: f32,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            peer_id: peer_id.into(),
            new_trust,
            trust_delta,
            timestamp: Utc::now(),
        }
    }
}
