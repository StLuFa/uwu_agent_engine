//! StateSnapshot + MVCC versioning

use crate::{LongTermWS, MidTermWS, ShortTermWS};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// MVCC 只读快照 —— Sidecar 消费用
///
/// 主进程写入时不阻塞 Sidecar 读取。
/// snapshot_version = max(short, mid, long).version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// 全局版本号 = max(short_term.version, mid_term.version, long_term.version)
    pub snapshot_version: u64,
    /// 短程状态
    pub short_term: ShortTermWS,
    /// 中程状态
    pub mid_term: MidTermWS,
    /// 长程状态
    pub long_term: LongTermWS,
    /// 快照时间
    pub taken_at: DateTime<Utc>,
}

impl StateSnapshot {
    /// 从三层独立状态创建快照
    pub fn new(
        short_term: ShortTermWS,
        mid_term: MidTermWS,
        long_term: LongTermWS,
    ) -> Self {
        let snapshot_version = short_term
            .version
            .max(mid_term.version)
            .max(long_term.version);
        Self {
            snapshot_version,
            short_term,
            mid_term,
            long_term,
            taken_at: Utc::now(),
        }
    }
}
