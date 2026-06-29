//! StateCheckpoint + checkpoint/rollback

use crate::state::AgentState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub fn from_state(state: &AgentState) -> Self {
        let data = serde_json::to_vec(state).expect("AgentState must be serializable");
        Self {
            state_id: state.state_id.0.clone(),
            taken_at: Utc::now(),
            data,
        }
    }

    /// 从检查点恢复 AgentState
    pub fn rollback(&self) -> AgentState {
        serde_json::from_slice(&self.data).expect("Checkpoint data must be valid AgentState JSON")
    }
}

/// 便捷函数 —— 与 lib.rs re-export 匹配
pub fn rollback(checkpoint: &StateCheckpoint) -> AgentState {
    checkpoint.rollback()
}
