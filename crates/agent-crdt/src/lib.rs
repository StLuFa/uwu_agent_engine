//! # agent-crdt
//!
//! CRDT (Conflict-free Replicated Data Types) for multi-agent state merging.
//!
//! ## Implemented types
//!
//! | Type | Merge | Use case |
//! |---|---|---|
//! | `GCounter` | element-wise max | tracking total task count per agent |
//! | `PNCounter` | P + N via two GCounters | increment/decrement counters |
//! | `LWWRegister<T>` | max-clock wins | shared values with versioning |
//! | `ORSet<T>` | add-wins with tombstones | shared sets (tags, capabilities) |
//! | `VectorClock` | entry-wise max | partial ordering of events |

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

// ===========================================================================
// Vector Clock
// ===========================================================================

/// Vector clock — `node_id → counter` map, tracks causal ordering of events.
///
/// # Partial order
/// - `a < b` if for all nodes `a[i] <= b[i]` and at least one node `a[i] < b[i]`
/// - `a || b` (concurrent) if neither dominates the other
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    pub entries: HashMap<String, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Increment the counter for this node.
    pub fn increment(&mut self, node_id: &str) {
        *self.entries.entry(node_id.to_string()).or_insert(0) += 1;
    }

    /// Get the counter for a specific node.
    pub fn get(&self, node_id: &str) -> u64 {
        self.entries.get(node_id).copied().unwrap_or(0)
    }

    /// Merge two vector clocks (entry-wise max).
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.entries.clone();
        for (k, &v) in &other.entries {
            let entry = merged.entry(k.clone()).or_insert(0);
            *entry = (*entry).max(v);
        }
        Self { entries: merged }
    }

    /// Check if self happens-before other (strict partial order).
    pub fn happens_before(&self, other: &Self) -> bool {
        let all_keys: HashSet<&String> =
            self.entries.keys().chain(other.entries.keys()).collect();
        let mut at_least_one_less = false;
        for k in all_keys {
            let a = self.entries.get(k).copied().unwrap_or(0);
            let b = other.entries.get(k).copied().unwrap_or(0);
            if a > b { return false; }
            if a < b { at_least_one_less = true; }
        }
        at_least_one_less
    }

    /// Check if two clocks are concurrent (neither dominates the other).
    pub fn concurrent(&self, other: &Self) -> bool {
        !self.happens_before(other) && !other.happens_before(self)
    }
}

impl Default for VectorClock {
    fn default() -> Self { Self::new() }
}

// ===========================================================================
// GCounter — Grow-only Counter
// ===========================================================================

/// Grow-only counter: each node tracks its own count, merge = element-wise max.
///
/// ```ignore
/// let mut c1 = GCounter::new();
/// c1.inc("A", 3);
/// c1.inc("A", 2); // A = 5
/// let mut c2 = GCounter::new();
/// c2.inc("B", 4); // B = 4
/// let merged = c1.merge(&c2);
/// assert_eq!(merged.value(), 9); // 5 + 4
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCounter {
    counts: HashMap<String, u64>,
}

impl GCounter {
    pub fn new() -> Self { Self { counts: HashMap::new() } }

    /// Increment a node's count.
    pub fn inc(&mut self, node_id: &str, delta: u64) {
        *self.counts.entry(node_id.to_string()).or_insert(0) += delta;
    }

    /// Total value across all nodes.
    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Merge two GCounters (element-wise max).
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.counts.clone();
        for (k, &v) in &other.counts {
            let entry = merged.entry(k.clone()).or_insert(0);
            *entry = (*entry).max(v);
        }
        Self { counts: merged }
    }
}

impl Default for GCounter {
    fn default() -> Self { Self::new() }
}

// ===========================================================================
// PNCounter — Positive-Negative Counter
// ===========================================================================

/// PN counter: uses two GCounters internally (P = increments, N = decrements).
/// Value = P.value() - N.value()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PNCounter {
    p: GCounter,
    n: GCounter,
}

impl PNCounter {
    pub fn new() -> Self { Self { p: GCounter::new(), n: GCounter::new() } }

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
    fn default() -> Self { Self::new() }
}

// ===========================================================================
// LWWRegister — Last-Writer-Wins Register
// ===========================================================================

/// LWW register: stores a value with a scalar clock. Merge = max-clock wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LWWRegister<T: Clone + PartialEq> {
    pub value: T,
    pub clock: u64,
    pub node_id: String,
}

impl<T: Clone + PartialEq> LWWRegister<T> {
    pub fn new(value: T, clock: u64, node_id: impl Into<String>) -> Self {
        Self { value, clock, node_id: node_id.into() }
    }

    /// Set a new value (only if clock advances).
    pub fn set(&mut self, value: T, clock: u64) {
        if clock > self.clock {
            self.value = value;
            self.clock = clock;
        }
    }

    /// Merge two registers: higher clock wins; tie → self wins.
    pub fn merge(&self, other: &Self) -> Self {
        if other.clock > self.clock
            || (other.clock == self.clock && other.node_id > self.node_id)
        {
            other.clone()
        } else {
            self.clone()
        }
    }
}

// ===========================================================================
// ORSet — Observed-Remove Set
// ===========================================================================

/// Element in an ORSet with a unique tag (prevents duplicate removal).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct TaggedElement<T: Clone + Eq + Hash> {
    value: T,
    tag: String,
}

/// Observed-Remove Set: add-wins, tracks removed elements via tombstones.
///
/// Each `add` creates a unique tag. `remove` records the currently-visible
/// tags in a tombstone set. On merge, an element is visible iff it has at
/// least one tag NOT in the merged tombstone set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ORSet<T: Clone + Eq + Hash> {
    /// All added elements (with unique tags).
    added: HashSet<TaggedElement<T>>,
    /// Tombstones: tags that have been removed.
    tombstones: HashSet<String>,
}

impl<T: Clone + Eq + Hash> ORSet<T> {
    pub fn new() -> Self {
        Self { added: HashSet::new(), tombstones: HashSet::new() }
    }

    /// Add a value with a unique tag. Tag should be globally unique (UUID).
    pub fn add(&mut self, value: T, tag: impl Into<String>) {
        self.added.insert(TaggedElement { value, tag: tag.into() });
    }

    /// Remove a value by marking all currently visible tags as removed.
    pub fn remove(&mut self, value: &T) {
        for elem in &self.added {
            if &elem.value == value {
                self.tombstones.insert(elem.tag.clone());
            }
        }
    }

    /// Check if a value is in the set.
    pub fn contains(&self, value: &T) -> bool {
        self.elements().contains(value)
    }

    /// All currently visible elements.
    pub fn elements(&self) -> HashSet<T> {
        self.added
            .iter()
            .filter(|e| !self.tombstones.contains(&e.tag))
            .map(|e| e.value.clone())
            .collect()
    }

    /// Number of visible elements.
    pub fn len(&self) -> usize {
        self.added.iter().filter(|e| !self.tombstones.contains(&e.tag)).count()
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Merge two ORSets: union added, union tombstones. Add-wins.
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged_added = self.added.clone();
        for elem in &other.added {
            merged_added.insert(elem.clone());
        }
        let mut merged_tombstones = self.tombstones.clone();
        for t in &other.tombstones {
            merged_tombstones.insert(t.clone());
        }
        Self { added: merged_added, tombstones: merged_tombstones }
    }
}

impl<T: Clone + Eq + Hash> Default for ORSet<T> {
    fn default() -> Self { Self::new() }
}

// ===========================================================================
// CRDT Store trait
// ===========================================================================

/// CRDT Merge trait — any type that can merge with another copy.
pub trait CRDTMerge {
    fn merge(&self, other: &Self) -> Self;
}

impl CRDTMerge for GCounter {
    fn merge(&self, other: &Self) -> Self { self.merge(other) }
}

impl CRDTMerge for PNCounter {
    fn merge(&self, other: &Self) -> Self { self.merge(other) }
}

impl<T: Clone + PartialEq> CRDTMerge for LWWRegister<T> {
    fn merge(&self, other: &Self) -> Self { self.merge(other) }
}

impl<T: Clone + Eq + Hash> CRDTMerge for ORSet<T> {
    fn merge(&self, other: &Self) -> Self { self.merge(other) }
}

impl CRDTMerge for VectorClock {
    fn merge(&self, other: &Self) -> Self { self.merge(other) }
}

/// Generic merge helper for any CRDTMerge type.
pub fn merge<T: CRDTMerge>(local: &T, remote: &T) -> T {
    local.merge(remote)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- VectorClock ----

    #[test]
    fn vector_clock_increment_and_merge() {
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
    fn vector_clock_happens_before() {
        let mut a = VectorClock::new();
        a.increment("X"); // {X: 1}
        let mut b = a.clone();
        b.increment("Y"); // {X: 1, Y: 1}
        assert!(a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }

    #[test]
    fn vector_clock_concurrent() {
        let mut a = VectorClock::new();
        a.increment("A"); // {A: 1}
        let mut b = VectorClock::new();
        b.increment("B"); // {B: 1}
        assert!(a.concurrent(&b));
    }

    // ---- GCounter ----

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
        let mut c1 = GCounter::new(); c1.inc("A", 1);
        let mut c2 = GCounter::new(); c2.inc("B", 2);
        let mut c3 = GCounter::new(); c3.inc("C", 3);
        let m12_3 = c1.merge(&c2).merge(&c3);
        let m1_23 = c1.merge(&c2.merge(&c3));
        assert_eq!(m12_3.value(), m1_23.value());
    }

    // ---- PNCounter ----

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
        assert_eq!(merged.value(), 10); // (10+5) - (4+1) = 10
    }

    // ---- LWWRegister ----

    #[test]
    fn lww_register_set_higher_clock() {
        let mut r = LWWRegister::new("v1", 1, "A");
        r.set("v2", 3);
        assert_eq!(r.value, "v2");
    }

    #[test]
    fn lww_register_ignore_lower_clock() {
        let mut r = LWWRegister::new("v1", 5, "A");
        r.set("v2", 3); // lower clock — ignored
        assert_eq!(r.value, "v1");
    }

    #[test]
    fn lww_register_merge_higher_clock_wins() {
        let r1 = LWWRegister::new("local", 1, "A");
        let r2 = LWWRegister::new("remote", 2, "B");
        let merged = r1.merge(&r2);
        assert_eq!(merged.value, "remote");
    }

    // ---- ORSet ----

    #[test]
    fn orset_add_and_contains() {
        let mut set = ORSet::new();
        set.add("item-1", "tag-1");
        set.add("item-2", "tag-2");
        assert!(set.contains(&"item-1"));
        assert!(set.contains(&"item-2"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn orset_remove_and_re_add() {
        let mut set = ORSet::new();
        set.add("item", "tag-1");
        set.remove(&"item");
        assert!(!set.contains(&"item"));
        // Re-add with new tag — add-wins
        set.add("item", "tag-2");
        assert!(set.contains(&"item"));
    }

    #[test]
    fn orset_merge_add_wins() {
        let mut s1 = ORSet::new();
        s1.add("shared", "tag-1");
        s1.remove(&"shared");

        let mut s2 = ORSet::new();
        s2.add("shared", "tag-2");

        // Merge: tag-1 is tombstoned but tag-2 is visible → add-wins
        let merged = s1.merge(&s2);
        assert!(merged.contains(&"shared"));
    }

    #[test]
    fn orset_merge_concurrent_removes() {
        let mut s1 = ORSet::new();
        s1.add("x", "t1");

        let mut s2 = ORSet::new();
        s2.add("x", "t2");

        let mut merged = s1.merge(&s2);
        assert_eq!(merged.len(), 2); // two tags, both visible

        merged.remove(&"x");
        assert!(!merged.contains(&"x"));
    }

    // ---- CRDTMerge blanket trait ----

    #[test]
    fn merge_helper_function() {
        let c1 = GCounter::new();
        let mut c2 = GCounter::new();
        c2.inc("X", 42);
        let result = merge(&c1, &c2);
        assert_eq!(result.value(), 42);
    }
}
