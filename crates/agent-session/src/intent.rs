//! IntentTracker

use serde::{Deserialize, Serialize};

/// 追踪用户意图在多个 turn 间的变化
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntentTracker {
    /// 当前推断的意图
    pub current_intent: Option<String>,
    /// 上一轮意图
    pub previous_intent: Option<String>,
    /// 意图是否发生了变化
    pub intent_changed: bool,
    /// 总轮数
    pub total_turns: u64,
    /// 连续相同意图的轮数
    pub consecutive_same_intent: u64,
}

impl IntentTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 更新意图追踪
    pub fn update(&mut self, new_intent: Option<String>) {
        self.previous_intent = self.current_intent.take();
        self.current_intent = new_intent;
        // First update (no previous) is never a "change"
        self.intent_changed = self.previous_intent.is_some()
            && self.current_intent != self.previous_intent;
        self.total_turns += 1;

        if self.intent_changed {
            self.consecutive_same_intent = 0;
        } else {
            self.consecutive_same_intent += 1;
        }
    }

    /// 用户是否在重复同样的意图（可能陷入循环）
    pub fn is_stuck(&self, threshold: u64) -> bool {
        self.consecutive_same_intent >= threshold
    }

    /// 推断意图（基于用户输入文本的简单启发式）
    pub fn infer(&self, user_input: &str) -> Option<String> {
        let lower = user_input.to_lowercase();
        if lower.contains("search") || lower.contains("find") || lower.contains("查找") {
            Some("search".into())
        } else if lower.contains("create") || lower.contains("make") || lower.contains("创建") {
            Some("create".into())
        } else if lower.contains("delete") || lower.contains("remove") || lower.contains("删除") {
            Some("delete".into())
        } else if lower.contains("help") || lower.contains("帮助") {
            Some("help".into())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_change_detection() {
        let mut tracker = IntentTracker::new();
        tracker.update(Some("search".into()));
        assert!(!tracker.intent_changed);

        tracker.update(Some("delete".into()));
        assert!(tracker.intent_changed);
    }

    #[test]
    fn consecutive_same_intent_counting() {
        let mut tracker = IntentTracker::new();
        tracker.update(Some("search".into()));
        tracker.update(Some("search".into()));
        tracker.update(Some("search".into()));
        assert_eq!(tracker.consecutive_same_intent, 3);
        assert!(tracker.is_stuck(3));
    }

    #[test]
    fn infer_search_intent() {
        let tracker = IntentTracker::new();
        assert_eq!(tracker.infer("please find the document"), Some("search".into()));
    }
}
