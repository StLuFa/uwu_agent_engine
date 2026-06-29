//! ConversationHistory

use crate::turn::ConversationTurn;
use serde::{Deserialize, Serialize};

/// 对话历史 —— 最近 N 轮完整记录
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationHistory {
    turns: Vec<ConversationTurn>,
    /// 总 token 消耗
    total_tokens: u64,
    /// 总 Reaction 命中次数
    total_reaction_hits: u64,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加一轮对话
    pub fn push(&mut self, turn: ConversationTurn) {
        self.total_tokens += turn.tokens_used;
        if turn.reaction_hit {
            self.total_reaction_hits += 1;
        }
        self.turns.push(turn);
    }

    /// 最近 N 轮
    pub fn recent(&self, n: usize) -> Vec<&ConversationTurn> {
        self.turns.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
    }

    /// 总轮数
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// 总 token 消耗
    pub fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    /// Reaction 命中率
    pub fn reaction_hit_rate(&self) -> f32 {
        if self.turns.is_empty() {
            return 0.0;
        }
        self.total_reaction_hits as f32 / self.turns.len() as f32
    }

    /// 所有 turn 迭代器
    pub fn iter(&self) -> impl Iterator<Item = &ConversationTurn> {
        self.turns.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_tracks_tokens() {
        let mut history = ConversationHistory::new();
        let turn = ConversationTurn::new(1, "hi", "hello", 100, "Proceed");
        history.push(turn);
        assert_eq!(history.total_tokens(), 100);
    }

    #[test]
    fn reaction_hit_rate_calculation() {
        let mut history = ConversationHistory::new();
        history.push(ConversationTurn::new(1, "a", "b", 0, "Proceed").with_reaction());
        history.push(ConversationTurn::new(2, "c", "d", 50, "Proceed"));
        assert!((history.reaction_hit_rate() - 0.5).abs() < 0.001);
    }
}
