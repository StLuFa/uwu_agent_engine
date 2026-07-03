//! Last-Writer-Wins Map —— 每键独立 LWW。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;

/// LWW-Map：每个键独立 LWW（`(clock, node_id)` 词典序高者胜）。
///
/// 用途：`Profile` / `Preferences` 等键值配置类 MemoryClass；也可用于任意
/// `key → value` 且不需要"元素级删除追加"语义的场景（那种用
/// [`ORSet`](super::ORSet)）。
///
/// **删除语义**：`remove` 写入墓碑（值 = `None`, 时钟前进）。合并时墓碑与写
/// 按同一 LWW 规则竞争；同时钟 `set` 与 `remove` 由 `node_id` 决出胜负。
/// 新 `set` 只要时钟前进就复活该键。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LwwMap<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone + PartialEq,
{
    entries: HashMap<K, LwwEntry<V>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LwwEntry<V: Clone + PartialEq> {
    /// `None` = 墓碑（删除）。
    value: Option<V>,
    clock: u64,
    node_id: String,
}

impl<V: Clone + PartialEq> LwwEntry<V> {
    /// LWW 比较：高时钟胜；平手 → node_id 大者胜。
    fn dominates(&self, other: &Self) -> bool {
        self.clock > other.clock
            || (self.clock == other.clock && self.node_id > other.node_id)
    }
}

impl<K, V> LwwMap<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone + PartialEq,
{
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// 写入 key。仅当 `(clock, node_id)` 严格支配已有 entry 时生效。
    pub fn set(&mut self, key: K, value: V, clock: u64, node_id: impl Into<String>) {
        self.upsert(key, LwwEntry { value: Some(value), clock, node_id: node_id.into() });
    }

    /// 删除 key（写墓碑）。仅当 `(clock, node_id)` 严格支配已有 entry 时生效。
    pub fn remove(&mut self, key: K, clock: u64, node_id: impl Into<String>) {
        self.upsert(key, LwwEntry { value: None, clock, node_id: node_id.into() });
    }

    fn upsert(&mut self, key: K, candidate: LwwEntry<V>) {
        match self.entries.get_mut(&key) {
            Some(cur) => {
                if candidate.dominates(cur) {
                    *cur = candidate;
                }
            }
            None => {
                self.entries.insert(key, candidate);
            }
        }
    }

    /// 读取 key（墓碑视为不存在）。
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key).and_then(|e| e.value.as_ref())
    }

    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// 遍历可见 (key, value)。
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries
            .iter()
            .filter_map(|(k, e)| e.value.as_ref().map(|v| (k, v)))
    }

    /// 可见键数量（不含墓碑）。
    pub fn len(&self) -> usize {
        self.entries.values().filter(|e| e.value.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 合并：每 key 取 LWW 胜者。
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.entries.clone();
        merged.reserve(other.entries.len());
        for (k, e) in &other.entries {
            match merged.get_mut(k) {
                Some(cur) => {
                    if e.dominates(cur) {
                        *cur = e.clone();
                    }
                }
                None => {
                    merged.insert(k.clone(), e.clone());
                }
            }
        }
        Self { entries: merged }
    }
}

impl<K, V> Default for LwwMap<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone + PartialEq,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove() {
        let mut m: LwwMap<String, String> = LwwMap::new();
        m.set("theme".into(), "dark".into(), 1, "A");
        m.set("lang".into(), "zh".into(), 1, "A");
        assert_eq!(m.get(&"theme".into()), Some(&"dark".to_string()));
        assert_eq!(m.len(), 2);

        m.remove("theme".into(), 2, "A");
        assert!(!m.contains(&"theme".into()));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn lower_clock_ignored() {
        let mut m: LwwMap<String, String> = LwwMap::new();
        m.set("k".into(), "v1".into(), 5, "A");
        m.set("k".into(), "v2".into(), 3, "A");
        assert_eq!(m.get(&"k".into()), Some(&"v1".to_string()));
    }

    #[test]
    fn tie_break_by_node_id() {
        let mut a: LwwMap<String, String> = LwwMap::new();
        let mut b: LwwMap<String, String> = LwwMap::new();
        a.set("k".into(), "from-a".into(), 1, "A");
        b.set("k".into(), "from-b".into(), 1, "B");
        let m = a.merge(&b);
        assert_eq!(m.get(&"k".into()), Some(&"from-b".to_string()));
    }

    #[test]
    fn merge_disjoint_keys() {
        let mut a: LwwMap<String, i32> = LwwMap::new();
        let mut b: LwwMap<String, i32> = LwwMap::new();
        a.set("x".into(), 1, 1, "A");
        b.set("y".into(), 2, 1, "B");
        let m = a.merge(&b);
        assert_eq!(m.get(&"x".into()), Some(&1));
        assert_eq!(m.get(&"y".into()), Some(&2));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn tombstone_beats_older_set() {
        let mut a: LwwMap<String, String> = LwwMap::new();
        let mut b: LwwMap<String, String> = LwwMap::new();
        a.set("k".into(), "v".into(), 1, "A");
        b.remove("k".into(), 2, "B");
        assert!(!a.merge(&b).contains(&"k".into()));
        assert!(!b.merge(&a).contains(&"k".into()));
    }

    #[test]
    fn re_add_after_remove() {
        let mut m: LwwMap<String, String> = LwwMap::new();
        m.set("k".into(), "v1".into(), 1, "A");
        m.remove("k".into(), 2, "A");
        m.set("k".into(), "v2".into(), 3, "A");
        assert_eq!(m.get(&"k".into()), Some(&"v2".to_string()));
    }

    #[test]
    fn idempotent_commutative_associative() {
        let mut a: LwwMap<String, i32> = LwwMap::new();
        a.set("x".into(), 1, 1, "A");
        let mut b: LwwMap<String, i32> = LwwMap::new();
        b.set("y".into(), 2, 1, "B");
        let mut c: LwwMap<String, i32> = LwwMap::new();
        c.set("x".into(), 9, 2, "C");

        let aa = a.merge(&a);
        assert_eq!(aa.get(&"x".into()), Some(&1));

        let ab = a.merge(&b);
        let ba = b.merge(&a);
        assert_eq!(ab.get(&"x".into()), ba.get(&"x".into()));
        assert_eq!(ab.get(&"y".into()), ba.get(&"y".into()));

        let abc = a.merge(&b).merge(&c);
        let a_bc = a.merge(&b.merge(&c));
        assert_eq!(abc.get(&"x".into()), a_bc.get(&"x".into()));
        assert_eq!(abc.get(&"x".into()), Some(&9));
    }
}
