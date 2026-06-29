//! ReactionStats —— hits/misses 无锁计数器

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// 反应层统计 —— 记录拦截命中/未命中次数
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ReactionStats {
    /// 规则命中次数（短路返回 Hit 的次数）
    pub hits: AtomicU64,
    /// 规则未命中次数（全部规则未匹配的次数）
    pub misses: AtomicU64,
}

impl ReactionStats {
    pub fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// 总拦截次数 = hits + misses
    pub fn total(&self) -> u64 {
        self.hits.load(Ordering::Relaxed) + self.misses.load(Ordering::Relaxed)
    }

    /// 命中率 [0.0, 1.0]，无调用时返回 0.0
    pub fn hit_rate(&self) -> f32 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.hits.load(Ordering::Relaxed) as f32 / total as f32
    }
}

// AtomicU64 不是 Clone，手动实现
impl Clone for ReactionStats {
    fn clone(&self) -> Self {
        Self {
            hits: AtomicU64::new(self.hits.load(Ordering::Relaxed)),
            misses: AtomicU64::new(self.misses.load(Ordering::Relaxed)),
        }
    }
}
