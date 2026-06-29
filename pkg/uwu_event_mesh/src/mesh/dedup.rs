//! Bounded-window idempotency dedup. O(1) check + insert.

use std::collections::{HashSet, VecDeque};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(super) struct DedupRing {
    cap: usize,
    set: HashSet<u64>,
    queue: VecDeque<u64>,
}

impl DedupRing {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            set: HashSet::with_capacity(cap.min(1024)),
            queue: VecDeque::with_capacity(cap.min(1024)),
        }
    }

    /// Returns true if `(topic, key)` was seen recently.
    pub fn check_and_insert(&mut self, topic: &str, key: &str) -> bool {
        let mut hasher = DefaultHasher::new();
        topic.hash(&mut hasher);
        0u8.hash(&mut hasher); // separator
        key.hash(&mut hasher);
        let h = hasher.finish();

        if !self.set.insert(h) {
            return true;
        }
        if self.queue.len() == self.cap {
            if let Some(old) = self.queue.pop_front() {
                self.set.remove(&old);
            }
        }
        self.queue.push_back(h);
        false
    }
}
