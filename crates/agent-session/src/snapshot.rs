//! SessionSnapshot

use agent_state::StateSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Session 快照 —— 供 Sidecar 只读消费
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub state_snapshot: StateSnapshot,
    pub persona_version: u64,
    pub turn_count: usize,
    pub total_tokens: u64,
    pub taken_at: DateTime<Utc>,
}

impl SessionSnapshot {
    pub fn new(
        session_id: impl Into<String>,
        state_snapshot: StateSnapshot,
        persona_version: u64,
        turn_count: usize,
        total_tokens: u64,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            state_snapshot,
            persona_version,
            turn_count,
            total_tokens,
            taken_at: Utc::now(),
        }
    }
}
