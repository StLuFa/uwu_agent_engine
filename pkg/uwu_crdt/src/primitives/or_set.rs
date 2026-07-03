//! Observed-Remove Set —— add-wins 集合。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// 观察-移除集合：add-wins，用墓碑记录已删。
///
/// 每次 `add` 产生唯一 tag；`remove` 把该值当前可见 tag 记入墓碑。
/// 元素可见当且仅当它至少有一个 tag 不在墓碑集内。
///
/// # 存储布局（性能）
/// `added` 以 `value → {tag}` 索引，使 [`contains`](Self::contains) /
/// [`remove`](Self::remove) 为 O(该值的 tag 数)，而非全表扫描；
/// [`len`](Self::len) 亦按值聚合判定可见性。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ORSet<T: Clone + Eq + Hash> {
    /// value → 该值的全部 tag。
    added: HashMap<T, HashSet<String>>,
    /// 已删除的 tag。
    tombstones: HashSet<String>,
}

impl<T: Clone + Eq + Hash> ORSet<T> {
    pub fn new() -> Self {
        Self { added: HashMap::new(), tombstones: HashSet::new() }
    }

    /// 加入一个值，tag 应全局唯一（UUID）。
    pub fn add(&mut self, value: T, tag: impl Into<String>) {
        self.added.entry(value).or_default().insert(tag.into());
    }

    /// 移除：把该值当前所有可见 tag 标记为墓碑。O(该值的 tag 数)。
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.added.get(value) {
            self.tombstones.reserve(tags.len());
            for t in tags {
                self.tombstones.insert(t.clone());
            }
        }
    }

    /// 是否可见：存在至少一个未被墓碑覆盖的 tag。O(该值的 tag 数)。
    pub fn contains(&self, value: &T) -> bool {
        self.added
            .get(value)
            .map(|tags| tags.iter().any(|t| !self.tombstones.contains(t)))
            .unwrap_or(false)
    }

    /// 当前可见元素。
    pub fn elements(&self) -> HashSet<T> {
        self.added
            .iter()
            .filter(|(_, tags)| tags.iter().any(|t| !self.tombstones.contains(t)))
            .map(|(v, _)| v.clone())
            .collect()
    }

    /// 可见元素数量。
    pub fn len(&self) -> usize {
        self.added
            .values()
            .filter(|tags| tags.iter().any(|t| !self.tombstones.contains(t)))
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 合并：added 按值并 tag 集，tombstones 取并集。add-wins。
    pub fn merge(&self, other: &Self) -> Self {
        let mut added = self.added.clone();
        added.reserve(other.added.len());
        for (v, tags) in &other.added {
            added.entry(v.clone()).or_default().extend(tags.iter().cloned());
        }
        let mut tombstones = self.tombstones.clone();
        tombstones.extend(other.tombstones.iter().cloned());
        Self { added, tombstones }
    }
}

impl<T: Clone + Eq + Hash> Default for ORSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_contains() {
        let mut set = ORSet::new();
        set.add("item-1", "tag-1");
        set.add("item-2", "tag-2");
        assert!(set.contains(&"item-1"));
        assert!(set.contains(&"item-2"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn remove_and_re_add() {
        let mut set = ORSet::new();
        set.add("item", "tag-1");
        set.remove(&"item");
        assert!(!set.contains(&"item"));
        set.add("item", "tag-2");
        assert!(set.contains(&"item"));
    }

    #[test]
    fn merge_add_wins() {
        let mut s1 = ORSet::new();
        s1.add("shared", "tag-1");
        s1.remove(&"shared");

        let mut s2 = ORSet::new();
        s2.add("shared", "tag-2");

        let merged = s1.merge(&s2);
        assert!(merged.contains(&"shared"));
    }

    #[test]
    fn merge_concurrent_removes() {
        let mut s1 = ORSet::new();
        s1.add("x", "t1");

        let mut s2 = ORSet::new();
        s2.add("x", "t2");

        let mut merged = s1.merge(&s2);
        assert_eq!(merged.len(), 2 - 1); // 同值 x，两 tag 归一到一个可见元素
        assert!(merged.contains(&"x"));

        merged.remove(&"x");
        assert!(!merged.contains(&"x"));
    }

    #[test]
    fn merge_idempotent_commutative() {
        let mut s1 = ORSet::new();
        s1.add("a", "t1");
        let mut s2 = ORSet::new();
        s2.add("b", "t2");
        let m12 = s1.merge(&s2);
        let m21 = s2.merge(&s1);
        assert_eq!(m12.len(), m21.len());
        // 幂等
        assert_eq!(m12.merge(&m12).len(), m12.len());
    }
}
