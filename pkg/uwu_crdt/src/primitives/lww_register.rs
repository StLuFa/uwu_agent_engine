//! Last-Writer-Wins 寄存器。

use serde::{Deserialize, Serialize};

/// LWW 寄存器：值 + 标量时钟，合并 = 高时钟胜。
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

    /// 设新值（仅当时钟前进）。
    pub fn set(&mut self, value: T, clock: u64) {
        if clock > self.clock {
            self.value = value;
            self.clock = clock;
        }
    }

    /// 合并：高时钟胜；平手 → node_id 大者胜。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_higher_clock() {
        let mut r = LWWRegister::new("v1", 1, "A");
        r.set("v2", 3);
        assert_eq!(r.value, "v2");
    }

    #[test]
    fn ignore_lower_clock() {
        let mut r = LWWRegister::new("v1", 5, "A");
        r.set("v2", 3);
        assert_eq!(r.value, "v1");
    }

    #[test]
    fn merge_higher_clock_wins() {
        let r1 = LWWRegister::new("local", 1, "A");
        let r2 = LWWRegister::new("remote", 2, "B");
        let merged = r1.merge(&r2);
        assert_eq!(merged.value, "remote");
    }

    #[test]
    fn merge_tie_break_by_node_id() {
        let r1 = LWWRegister::new("a", 1, "A");
        let r2 = LWWRegister::new("b", 1, "B");
        assert_eq!(r1.merge(&r2).value, "b");
        assert_eq!(r2.merge(&r1).value, "b");
    }
}
