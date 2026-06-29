//! StateDiff + compute_pred_error

use crate::mid::Fact;
use crate::state::AgentState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 两份 AgentState 之间的结构化差异
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateDiff {
    /// 实际中新增的事实（预测中不存在）
    pub facts_added: Vec<Fact>,
    /// 被修改的事实（key 相同，value 不同）
    pub facts_modified: Vec<Fact>,
    /// 被移除的事实（预测中存在，实际中不存在）
    pub facts_removed: Vec<Fact>,
}

impl StateDiff {
    /// 按 key 比较两份已知事实列表，O(n+m)
    pub fn from_states(predicted: &[Fact], actual: &[Fact]) -> Self {
        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut removed = Vec::new();

        // Build key→fact lookup for actual facts
        let actual_by_key: std::collections::HashMap<&str, &Fact> =
            actual.iter().map(|f| (f.key.as_str(), f)).collect();

        // Facts in predicted:
        // - key exists in actual with same value → no change
        // - key exists in actual with different value → modified
        // - key doesn't exist in actual → removed
        for p_fact in predicted {
            match actual_by_key.get(p_fact.key.as_str()) {
                Some(a_fact) if a_fact.value != p_fact.value => {
                    modified.push(p_fact.clone());
                }
                Some(_) => {
                    // Same key, same value — no change
                }
                None => {
                    removed.push(p_fact.clone());
                }
            }
        }

        // Facts in actual but not in predicted → added
        let predicted_keys: HashSet<&str> =
            predicted.iter().map(|f| f.key.as_str()).collect();
        for a_fact in actual {
            if !predicted_keys.contains(a_fact.key.as_str()) {
                added.push(a_fact.clone());
            }
        }

        Self {
            facts_added: added,
            facts_modified: modified,
            facts_removed: removed,
        }
    }

    /// JEPA 预测误差：diff 规模 / total_facts，范围 [0.0, 1.0]
    ///
    /// `pred_error = (|facts_added| + |facts_modified|) / max(|predicted|, |actual|, 1)`
    pub fn compute_pred_error(predicted: &AgentState, actual: &AgentState) -> f32 {
        let predicted_facts = &predicted.mid_term.known_facts;
        let actual_facts = &actual.mid_term.known_facts;
        let total = (predicted_facts.len().max(actual_facts.len())).max(1) as f32;
        let diff = Self::from_states(predicted_facts, actual_facts);
        let error = (diff.facts_added.len() + diff.facts_modified.len()) as f32 / total;
        error.clamp(0.0, 1.0)
    }

    /// 是否为空 diff
    pub fn is_empty(&self) -> bool {
        self.facts_added.is_empty()
            && self.facts_modified.is_empty()
            && self.facts_removed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentState;

    #[test]
    fn diff_added_facts() {
        let predicted = vec![];
        let actual = vec![Fact::new("color", "red", 1.0)];
        let diff = StateDiff::from_states(&predicted, &actual);
        assert_eq!(diff.facts_added.len(), 1);
        assert!(diff.facts_modified.is_empty());
        assert!(diff.facts_removed.is_empty());
    }

    #[test]
    fn diff_modified_facts() {
        let predicted = vec![Fact::new("color", "red", 1.0)];
        let actual = vec![Fact::new("color", "blue", 1.0)];
        let diff = StateDiff::from_states(&predicted, &actual);
        assert!(diff.facts_added.is_empty());
        assert_eq!(diff.facts_modified.len(), 1);
        assert!(diff.facts_removed.is_empty());
    }

    #[test]
    fn diff_removed_facts() {
        let predicted = vec![Fact::new("color", "red", 1.0)];
        let actual = vec![];
        let diff = StateDiff::from_states(&predicted, &actual);
        assert!(diff.facts_added.is_empty());
        assert!(diff.facts_modified.is_empty());
        assert_eq!(diff.facts_removed.len(), 1);
    }

    #[test]
    fn diff_same_facts_no_change() {
        let facts = vec![Fact::new("a", "1", 1.0), Fact::new("b", "2", 1.0)];
        let diff = StateDiff::from_states(&facts, &facts);
        assert!(diff.is_empty());
    }

    #[test]
    fn pred_error_zero_when_identical() {
        let facts = vec![Fact::new("a", "1", 1.0), Fact::new("b", "2", 1.0)];
        let mut p = AgentState::new();
        p.mid_term.known_facts = facts.clone();
        let mut a = AgentState::new();
        a.mid_term.known_facts = facts;

        let err = StateDiff::compute_pred_error(&p, &a);
        assert!((err - 0.0).abs() < 0.001);
    }

    #[test]
    fn pred_error_one_when_all_different() {
        let predicted = vec![Fact::new("a", "1", 1.0)];
        let actual = vec![Fact::new("b", "2", 1.0)];
        let mut p = AgentState::new();
        p.mid_term.known_facts = predicted;
        let mut a = AgentState::new();
        a.mid_term.known_facts = actual;

        let err = StateDiff::compute_pred_error(&p, &a);
        // 1 added + 1 removed, total=max(1,1)=1, err=1/1=1
        assert!((err - 1.0).abs() < 0.001);
    }

    #[test]
    fn pred_error_handles_empty_states() {
        let p = AgentState::new();
        let a = AgentState::new();
        let err = StateDiff::compute_pred_error(&p, &a);
        // Both empty, max len = 1, diff empty → 0/1 = 0
        assert!((err - 0.0).abs() < 0.001);
    }
}
