//! evaluate() → StateScore

use crate::state::AgentState;
use serde::{Deserialize, Serialize};

/// AgentState 综合评分
///
/// 三个维度：事实一致性 + 目标对齐 + 约束满足
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateScore {
    /// 综合评分 [0.0, 1.0]
    pub total: f32,
    /// 事实一致性：1.0 - 累积预测误差
    pub fact_consistency: f32,
    /// 目标对齐：任务进度占比例
    pub goal_alignment: f32,
    /// 约束满足：满足的约束比例
    pub constraint_satisfaction: f32,
}

impl StateScore {
    /// 对 State 进行综合评分
    ///
    /// 三等权融合：
    /// `total = (fact_consistency + goal_alignment + constraint_satisfaction) / 3`
    pub fn evaluate(state: &AgentState) -> Self {
        let fact_consistency = Self::compute_fact_consistency(state);
        let goal_alignment = Self::compute_goal_alignment(state);
        let constraint_satisfaction = Self::compute_constraint_satisfaction(state);

        let total = (fact_consistency + goal_alignment + constraint_satisfaction) / 3.0;

        Self {
            total: total.clamp(0.0, 1.0),
            fact_consistency,
            goal_alignment,
            constraint_satisfaction,
        }
    }

    /// 事实一致性 = 1.0 - accumulated_pred_error
    fn compute_fact_consistency(state: &AgentState) -> f32 {
        1.0 - state.long_term.accumulated_pred_error
    }

    /// 目标对齐 = 任务完成比例
    fn compute_goal_alignment(state: &AgentState) -> f32 {
        state.long_term.task_progress.fraction()
    }

    /// 约束满足 —— 启发式（confidence map 查找）
    ///
    /// 深度约束检查由 GuardLayer 负责。此处为快速近似。
    fn compute_constraint_satisfaction(state: &AgentState) -> f32 {
        if state.mid_term.active_constraints.is_empty() {
            return 1.0;
        }
        let satisfied = state
            .mid_term
            .active_constraints
            .iter()
            .filter(|c| state.confidence.get(&c.id) > 0.5)
            .count();
        satisfied as f32 / state.mid_term.active_constraints.len() as f32
    }
}

/// 便捷函数 —— 与 lib.rs re-export 匹配
pub fn evaluate(state: &AgentState) -> StateScore {
    StateScore::evaluate(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mid::Constraint;
    use crate::mid::ConstraintType;

    #[test]
    fn evaluate_empty_state() {
        let state = AgentState::new();
        let score = state.evaluate();
        // No accumulated error → fact_consistency = 1.0
        // No progress → goal_alignment = 0.0
        // No constraints → constraint_satisfaction = 1.0
        // total = (1.0 + 0.0 + 1.0) / 3.0 = 0.666...
        assert!((score.total - 2.0 / 3.0).abs() < 0.01);
        assert!((score.fact_consistency - 1.0).abs() < 0.01);
        assert!((score.goal_alignment - 0.0).abs() < 0.01);
        assert!((score.constraint_satisfaction - 1.0).abs() < 0.01);
    }

    #[test]
    fn evaluate_with_pred_error() {
        let mut state = AgentState::new();
        state.long_term.accumulated_pred_error = 0.3;
        let score = StateScore::evaluate(&state);
        assert!((score.fact_consistency - 0.7).abs() < 0.01);
    }

    #[test]
    fn evaluate_with_progress() {
        let mut state = AgentState::new();
        state.long_term.task_progress.subtasks_completed = 3;
        state.long_term.task_progress.subtasks_total = Some(5);
        let score = StateScore::evaluate(&state);
        assert!((score.goal_alignment - 0.6).abs() < 0.01);
    }

    #[test]
    fn evaluate_with_constraints() {
        let mut state = AgentState::new();
        let c = Constraint::new(
            "test constraint",
            ConstraintType::MustInclude {
                fact_key: "required".into(),
            },
        );
        state.mid_term.active_constraints.push(c.clone());
        // Without confidence set → constraint_satisfaction = 0/1 = 0.0
        let score = StateScore::evaluate(&state);
        assert!((score.constraint_satisfaction - 0.0).abs() < 0.01);

        // Set confidence above threshold → satisfied
        state.confidence.set(&c.id, 0.8);
        let score2 = StateScore::evaluate(&state);
        assert!((score2.constraint_satisfaction - 1.0).abs() < 0.01);
    }

    #[test]
    fn evaluate_perfect_state() {
        let mut state = AgentState::new();
        state.long_term.task_progress.subtasks_completed = 5;
        state.long_term.task_progress.subtasks_total = Some(5);
        // No pred error, all subtasks done, no constraints → perfect score
        let score = StateScore::evaluate(&state);
        assert!((score.total - 1.0).abs() < 0.01);
    }
}
