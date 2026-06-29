//! ConversationTurn

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 一轮对话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// Turn 序号
    pub turn_number: u64,
    /// 用户输入
    pub user_input: String,
    /// Agent 输出
    pub agent_output: String,
    /// 使用的 token 数
    pub tokens_used: u64,
    /// Reaction 是否命中（短路）
    pub reaction_hit: bool,
    /// MetaAction 决策
    pub meta_action: String,
    /// 是否成功
    pub success: bool,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

impl ConversationTurn {
    pub fn new(
        turn_number: u64,
        user_input: impl Into<String>,
        agent_output: impl Into<String>,
        tokens_used: u64,
        meta_action: impl Into<String>,
    ) -> Self {
        Self {
            turn_number,
            user_input: user_input.into(),
            agent_output: agent_output.into(),
            tokens_used,
            reaction_hit: false,
            meta_action: meta_action.into(),
            success: true,
            timestamp: Utc::now(),
        }
    }

    pub fn with_reaction(mut self) -> Self {
        self.reaction_hit = true;
        self.tokens_used = 0;
        self
    }

    pub fn with_failure(mut self) -> Self {
        self.success = false;
        self
    }
}
