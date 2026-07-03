//! 向量时钟 —— 事件因果序。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 向量时钟 —— `node_id → counter` 映射，跟踪事件因果序。
///
/// # 偏序
/// - `a < b`：对所有节点 `a[i] <= b[i]` 且至少一个 `a[i] < b[i]`
/// - `a || b`（并发）：互不支配
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    pub entries: HashMap<String, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// 递增本节点计数。
    pub fn increment(&mut self, node_id: &str) {
        // 命中已存在的键时不分配 String；仅缺失时插入。
        if let Some(v) = self.entries.get_mut(node_id) {
            *v += 1;
        } else {
            self.entries.insert(node_id.to_string(), 1);
        }
    }

    /// 读取指定节点计数。
    pub fn get(&self, node_id: &str) -> u64 {
        self.entries.get(node_id).copied().unwrap_or(0)
    }

    /// 合并两个向量时钟（entry-wise max）。
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.entries.clone();
        merged.reserve(other.entries.len());
        for (k, &v) in &other.entries {
            merged
                .entry(k.clone())
                .and_modify(|e| {
                    if v > *e {
                        *e = v;
                    }
                })
                .or_insert(v);
        }
        Self { entries: merged }
    }

    /// self 是否严格发生于 other 之前。
    ///
    /// 单次遍历，不分配中间键集合：先扫 self 的键，再补扫 other 独有的键。
    pub fn happens_before(&self, other: &Self) -> bool {
        let mut at_least_one_less = false;
        for (k, &a) in &self.entries {
            let b = other.get(k);
            if a > b {
                return false;
            }
            if a < b {
                at_least_one_less = true;
            }
        }
        // other 独有的键：self 计为 0，只要该键 > 0 即构成 a < b。
        if !at_least_one_less {
            for (k, &b) in &other.entries {
                if b > 0 && !self.entries.contains_key(k) {
                    at_least_one_less = true;
                    break;
                }
            }
        }
        at_least_one_less
    }

    /// 两个时钟是否并发（互不支配）。
    pub fn concurrent(&self, other: &Self) -> bool {
        !self.happens_before(other) && !other.happens_before(self)
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_and_merge() {
        let mut a = VectorClock::new();
        a.increment("A");
        a.increment("A");
        let mut b = VectorClock::new();
        b.increment("B");
        b.increment("B");
        b.increment("B");

        let merged = a.merge(&b);
        assert_eq!(merged.get("A"), 2);
        assert_eq!(merged.get("B"), 3);
    }

    #[test]
    fn happens_before() {
        let mut a = VectorClock::new();
        a.increment("X");
        let mut b = a.clone();
        b.increment("Y");
        assert!(a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }

    #[test]
    fn concurrent() {
        let mut a = VectorClock::new();
        a.increment("A");
        let mut b = VectorClock::new();
        b.increment("B");
        assert!(a.concurrent(&b));
    }

    #[test]
    fn identical_clocks_not_happens_before() {
        let mut a = VectorClock::new();
        a.increment("A");
        let b = a.clone();
        // 相等时钟：互不严格先于对方。
        assert!(!a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }
}
