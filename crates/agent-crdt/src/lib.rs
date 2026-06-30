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
    /// 合并两份状态：remote 胜出（适合无版本信息的简单场景）。
    ///
    /// 如果本地和远端相同，返回本地引用以避免不必要的 clone。
    pub fn merge<T: Clone + PartialEq>(local: &T, remote: &T) -> T {
        if local == remote {
            local.clone()
        } else {
            remote.clone()
        }
    }

    /// LWW (Last-Writer-Wins) 合并：时钟大的胜出。
    ///
    /// 时钟相等时 remote 胜出（打破平局）。
    pub fn merge_lww<T: Clone>(
        local: &T,
        local_version: &CRDTVersion,
        remote: &T,
        remote_version: &CRDTVersion,
    ) -> T {
        if remote_version.clock >= local_version.clock {
            remote.clone()
        } else {
            local.clone()
        }
    }
}

/// CRDT 状态版本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CRDTVersion {
    pub clock: u64,
    pub node_id: String,
}
