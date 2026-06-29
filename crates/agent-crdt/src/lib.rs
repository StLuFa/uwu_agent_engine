//! # agent-crdt
//!
//! CRDT 状态 —— 多 Agent 共享状态的无冲突合并。
//!
//! 用于 Collaboration 中多个 Agent 对同一份状态的并发修改。

use serde::{Deserialize, Serialize};

/// CRDT 存储 trait
pub trait CRDTStore: Send + Sync {
    type Key;
    type Value;

    fn merge(&self, key: &Self::Key, a: &Self::Value, b: &Self::Value) -> Self::Value;
}

/// 状态合并器 —— 将两份 State 无冲突合并
pub struct StateMerger;

impl StateMerger {
    /// 合并两份状态（先到先得 + 最后写入胜出）
    pub fn merge<T: Clone + PartialEq>(local: &T, remote: &T) -> T {
        // 简化实现：remote 胜出
        remote.clone()
    }
}

/// CRDT 状态版本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CRDTVersion {
    pub clock: u64,
    pub node_id: String,
}
