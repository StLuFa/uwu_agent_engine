//! 只增计数器 [`GCounter`] 与增减计数器 [`PNCounter`]。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 只增计数器：每个节点各记己数，合并 = element-wise max。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCounter {
    counts: HashMap<String, u64>,
    /// 缓存的合计值，写时增量维护，`value()` O(1)。
    total: u64,
}

impl GCounter {
    pub fn new() -> Self {
        Self { counts: HashMap::new(), total: 0 }
    }

    /// 递增某节点计数。
    pub fn inc(&mut self, node_id: &str, delta: u64) {
        if let Some(v) = self.counts.get_mut(node_id) {
            *v += delta;
        } else {
            self.counts.insert(node_id.to_string(), delta);
        }
        self.total += delta;
    }

    /// 全节点合计（O(1)）。
    pub fn value(&self) -> u64 {
        self.total
    }

    /// 合并（element-wise max）。
    pub fn merge(&self, other: &Self) -> Self {
        let mut counts = self.counts.clone();
        counts.reserve(other.counts.len());
        let mut total = self.total;
        for (k, &v) in &other.counts {
            match counts.get_mut(k) {
                Some(e) => {
                    if v > *e {
                        total += v - *e;
                        *e = v;
                    }
                }
                None => {
                    total += v;
                    counts.insert(k.clone(), v);
                }
            }
        }
        Self { counts, total }
    }
}

impl Default for GCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// PN 计数器：内部两个 GCounter（P = 增量，N = 减量），值 = P - N。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PNCounter {
    p: GCounter,
    n: GCounter,
}

impl PNCounter {
    pub fn new() -> Self {
        Self { p: GCounter::new(), n: GCounter::new() }
    }

    pub fn inc(&mut self, node_id: &str, delta: u64) {
        self.p.inc(node_id, delta);
    }

    pub fn dec(&mut self, node_id: &str, delta: u64) {
        self.n.inc(node_id, delta);
    }

    pub fn value(&self) -> i64 {
        self.p.value() as i64 - self.n.value() as i64
    }

    pub fn merge(&self, other: &Self) -> Self {
        Self {
            p: self.p.merge(&other.p),
            n: self.n.merge(&other.n),
        }
    }
}

impl Default for PNCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcounter_single_node() {
        let mut c = GCounter::new();
        c.inc("A", 5);
        c.inc("A", 3);
        assert_eq!(c.value(), 8);
    }

    #[test]
    fn gcounter_multi_node_merge() {
        let mut c1 = GCounter::new();
        c1.inc("A", 3);
        c1.inc("A", 2);
        let mut c2 = GCounter::new();
        c2.inc("B", 4);
        let merged = c1.merge(&c2);
        assert_eq!(merged.value(), 9);
    }

    #[test]
    fn gcounter_idempotent() {
        let mut c = GCounter::new();
        c.inc("A", 5);
        let merged = c.merge(&c);
        assert_eq!(merged.value(), 5);
    }

    #[test]
    fn gcounter_commutative() {
        let mut c1 = GCounter::new();
        c1.inc("A", 3);
        let mut c2 = GCounter::new();
        c2.inc("B", 7);
        let m12 = c1.merge(&c2);
        let m21 = c2.merge(&c1);
        assert_eq!(m12.value(), m21.value());
    }

    #[test]
    fn gcounter_associative() {
        let mut c1 = GCounter::new();
        c1.inc("A", 1);
        let mut c2 = GCounter::new();
        c2.inc("B", 2);
        let mut c3 = GCounter::new();
        c3.inc("C", 3);
        let m12_3 = c1.merge(&c2).merge(&c3);
        let m1_23 = c1.merge(&c2.merge(&c3));
        assert_eq!(m12_3.value(), m1_23.value());
    }

    #[test]
    fn gcounter_merge_keeps_max_not_sum_per_node() {
        // 同节点分别推进到 5 与 3，合并取 max=5，不是 8。
        let mut a = GCounter::new();
        a.inc("A", 5);
        let mut b = GCounter::new();
        b.inc("A", 3);
        assert_eq!(a.merge(&b).value(), 5);
    }

    #[test]
    fn pn_counter_inc_and_dec() {
        let mut c = PNCounter::new();
        c.inc("A", 10);
        c.dec("A", 3);
        assert_eq!(c.value(), 7);
    }

    #[test]
    fn pn_counter_merge() {
        let mut c1 = PNCounter::new();
        c1.inc("A", 10);
        c1.dec("A", 4);
        let mut c2 = PNCounter::new();
        c2.inc("B", 5);
        c2.dec("B", 1);
        let merged = c1.merge(&c2);
        assert_eq!(merged.value(), 10);
    }
}
