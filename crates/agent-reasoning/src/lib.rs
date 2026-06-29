//! # agent-reasoning
//!
//! 推理域 —— 消费 AgentState + fork() 推演沙盒 + Tree-of-Thought beam search。
//!
//! 作为 visual_script NodeDefinition 注册：`"reasoning.decide"`（Impure + Async）
//!
//! ## 推理策略（根据 TTS 信号切换）
//!
//! | TTSSignal | 策略 |
//! |---|---|
//! | Normal | ToT beam search（多候选推演 + 剪枝） |
//! | Degraded | 单步推理（禁用 ToT） |
//! | Urgent | 直接回答（禁止新工具调用） |
//! | Abort | 终止 |

mod reasoner;
mod sandbox;
mod strategies;
mod tot;
#[cfg(feature = "visual-script")]
pub mod vs_nodes;

pub use reasoner::{Decision, Reasoner};
pub use sandbox::SandboxEvaluator;
pub use strategies::ReasoningStrategy;
pub use tot::{ToTConfig, ToTExplorer};

use agent_state::AgentState;

/// 推理输入
#[derive(Debug, Clone)]
pub struct ReasoningInput {
    pub goal: String,
    pub state_snapshot: AgentState,
    pub persona_context: Option<String>,
    pub character_context: Option<String>,
}

/// 推理输出
#[derive(Debug, Clone)]
pub struct ReasoningOutput {
    pub decision: Decision,
    pub state_delta: agent_state::StateDiff,
    pub tokens_used: u64,
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use agent_types_core::{Action, ActionParams};

    #[test]
    fn decision_single_creates_one_action() {
        let action = Action::new("test", ActionParams::new());
        let decision = Decision::single(action.clone(), 0.9, "test");
        assert_eq!(decision.actions.len(), 1);
        assert_eq!(decision.best_action().unwrap().command, "test");
        assert!((decision.best_score() - 0.9).abs() < 0.001);
    }

    #[test]
    fn decision_multi_returns_best_first() {
        let a1 = Action::new("a", ActionParams::new());
        let a2 = Action::new("b", ActionParams::new());
        let decision = Decision::new(vec![a1.clone(), a2.clone()], vec![0.9, 0.5], "multi");
        assert_eq!(decision.best_action().unwrap().command, "a");
        assert!((decision.best_score() - 0.9).abs() < 0.001);
    }

    #[test]
    fn strategy_from_cost_covers_all_levels() {
        assert!(ReasoningStrategy::from_cost_remaining(0.8).allows_tot());
        assert!(!ReasoningStrategy::from_cost_remaining(0.3).allows_tot());
        assert!(!ReasoningStrategy::from_cost_remaining(0.1).allows_new_tools());
        assert!(ReasoningStrategy::from_cost_remaining(0.0).should_abort());
    }

    #[test]
    fn sandbox_evaluator_multi_candidate() {
        let state = AgentState::new();
        let actions = vec![
            Action::new("x", ActionParams::new()),
            Action::new("y", ActionParams::new()),
            Action::new("z", ActionParams::new()),
        ];
        let results = SandboxEvaluator::evaluate_candidates(&state, &actions);
        assert_eq!(results.len(), 3);
    }
}
