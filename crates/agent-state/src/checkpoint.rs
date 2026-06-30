//! StateCheckpoint + checkpoint/rollback

use crate::state::AgentState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Checkpoint error.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("serialize state failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("deserialize checkpoint failed: {0}")]
    Deserialize(String),
}

/// AgentState 的序列化检查点
///
/// 在执行可能有副作用的操作之前创建，崩溃后可恢复。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateCheckpoint {
    /// 原始 state_id
    pub state_id: String,
    /// 快照时间
    pub taken_at: DateTime<Utc>,
    /// JSON 序列化的完整 State 数据
    pub data: Vec<u8>,
}

impl StateCheckpoint {
    /// 从当前 State 创建检查点
    ///
    /// Panics only if AgentState contains a non-serializable type
    /// (programmer error, not a runtime condition).
    pub fn from_state(state: &AgentState) -> Self {
        let data = serde_json::to_vec(state)
            .expect("AgentState serialization failed — check for non-serializable fields");
        Self {
            state_id: state.state_id.0.clone(),
            taken_at: Utc::now(),
            data,
        }
    }

    /// 从检查点恢复 AgentState。
    ///
    /// Returns `Err` if the checkpoint data is corrupted or from an incompatible version.
    pub fn rollback(&self) -> Result<AgentState, CheckpointError> {
        serde_json::from_slice(&self.data)
            .map_err(|e| CheckpointError::Deserialize(format!("{e}")))
    }
}

/// 便捷函数 —— 与 lib.rs re-export 匹配
pub fn rollback(checkpoint: &StateCheckpoint) -> Result<AgentState, CheckpointError> {
    checkpoint.rollback()
}
