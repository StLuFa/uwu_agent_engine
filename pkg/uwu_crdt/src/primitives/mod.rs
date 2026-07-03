//! 状态型 CRDT 原语 —— 多 Agent 共享状态的无冲突合并。
//!
//! | 类型 | 合并策略 | 用途 |
//! |---|---|---|
//! | [`GCounter`] | element-wise max | 只增计数（任务总数） |
//! | [`PNCounter`] | P + N 两个 GCounter | 增减计数（活跃任务数） |
//! | [`LWWRegister`] | max-clock wins | 共享值（配置 / 信任分） |
//! | [`ORSet`] | add-wins + tombstones | 共享集合（capabilities / tags） |
//! | [`LwwMap`] | per-key max-clock wins | 键值配置（Profile / Preferences / Entities） |
//! | [`VectorClock`] | entry-wise max | 事件因果序（happens-before / concurrent） |
//!
//! 每个类型独立成模块；本文件只汇聚 [`CRDTMerge`] trait、[`merge`] 助手与 re-export。

mod counter;
mod lww_map;
mod lww_register;
mod or_set;
mod vector_clock;

pub use counter::{GCounter, PNCounter};
pub use lww_map::LwwMap;
pub use lww_register::LWWRegister;
pub use or_set::ORSet;
pub use vector_clock::VectorClock;

use std::hash::Hash;

// ===========================================================================
// CRDTMerge trait
// ===========================================================================

/// CRDT 合并 trait —— 任意可与同类副本合并的类型。
///
/// 实现须满足 CRDT 三律：**幂等** `a∨a = a`、**交换** `a∨b = b∨a`、
/// **结合** `(a∨b)∨c = a∨(b∨c)`。
pub trait CRDTMerge {
    fn merge(&self, other: &Self) -> Self;
}

impl CRDTMerge for GCounter {
    fn merge(&self, other: &Self) -> Self {
        GCounter::merge(self, other)
    }
}

impl CRDTMerge for PNCounter {
    fn merge(&self, other: &Self) -> Self {
        PNCounter::merge(self, other)
    }
}

impl<T: Clone + PartialEq> CRDTMerge for LWWRegister<T> {
    fn merge(&self, other: &Self) -> Self {
        LWWRegister::merge(self, other)
    }
}

impl<T: Clone + Eq + Hash> CRDTMerge for ORSet<T> {
    fn merge(&self, other: &Self) -> Self {
        ORSet::merge(self, other)
    }
}

impl CRDTMerge for VectorClock {
    fn merge(&self, other: &Self) -> Self {
        VectorClock::merge(self, other)
    }
}

impl<K, V> CRDTMerge for LwwMap<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone + PartialEq,
{
    fn merge(&self, other: &Self) -> Self {
        LwwMap::merge(self, other)
    }
}

/// 泛型合并助手。
pub fn merge<T: CRDTMerge>(local: &T, remote: &T) -> T {
    local.merge(remote)
}
