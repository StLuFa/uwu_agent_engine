//! Decision + Reasoner trait

use agent_state::AgentState;
use agent_types_core::Action;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 推理决策 —— 一轮推理的输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// 候选动作列表（按评分降序）
    pub actions: Vec<Action>,
    /// 每个候选动作的 State 评分
    pub scores: Vec<f32>,
    /// 推理过程的文本描述
    pub reasoning: String,
}

impl Decision {
    pub fn new(actions: Vec<Action>, scores: Vec<f32>, reasoning: impl Into<String>) -> Self {
        Self {
            actions,
            scores,
            reasoning: reasoning.into(),
        }
    }

    /// 仅一条动作的决策
    pub fn single(action: Action, score: f32, reasoning: impl Into<String>) -> Self {
        Self {
            actions: vec![action],
            scores: vec![score],
            reasoning: reasoning.into(),
        }
    }

    /// 获取最佳动作
    pub fn best_action(&self) -> Option<&Action> {
        self.actions.first()
    }

    /// 获取最佳评分
    pub fn best_score(&self) -> f32 {
        self.scores.first().copied().unwrap_or(0.0)
    }
}

/// 推理器 trait —— 消费 State，输出 Decision
#[async_trait]
pub trait Reasoner: Send + Sync {
    /// 执行推理
    async fn reason(
        &self,
        state: &AgentState,
        goal: &str,
        context: Option<&str>,
    ) -> Decision;
}
