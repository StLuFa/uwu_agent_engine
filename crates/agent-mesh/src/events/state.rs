//! StateSnapshotEvent

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// State 快照事件 —— Sidecar 消费
///
/// Topic: `"agent.state.snapshot"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshotEvent {
    /// 事件 ID
    pub event_id: String,
    /// 产生此事件的 Agent ID
    pub agent_id: String,
    /// State 快照的 JSON 表示
    pub snapshot_json: String,
    /// 快照版本号
    pub snapshot_version: u64,
    /// 事件时间
    pub timestamp: DateTime<Utc>,
}

impl StateSnapshotEvent {
    pub fn new(
        agent_id: impl Into<String>,
        snapshot_json: impl Into<String>,
        snapshot_version: u64,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            snapshot_json: snapshot_json.into(),
            snapshot_version,
            timestamp: Utc::now(),
        }
    }
}
