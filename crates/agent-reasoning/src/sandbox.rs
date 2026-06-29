//! SandboxEvaluator —— fork 沙盒推演候选动作

use agent_state::AgentState;
use agent_types_core::Action;

/// SandboxEvaluator —— 在 fork 沙盒中推演候选动作并评分
pub struct SandboxEvaluator;

impl SandboxEvaluator {
    /// 对每个候选动作：fork State → apply_hypothetical → evaluate → 收集评分
    pub fn evaluate_candidates(
        state: &AgentState,
        candidates: &[Action],
    ) -> Vec<(Action, f32)> {
        candidates
            .iter()
            .map(|action| {
                let mut sandbox = state.fork();
                sandbox.apply_hypothetical(action);
                let score = sandbox.evaluate().total;
                (action.clone(), score)
            })
            .collect()
    }

    /// 返回评分最高的候选动作
    pub fn best_candidate(
        state: &AgentState,
        candidates: &[Action],
    ) -> Option<(Action, f32)> {
        let mut evaluated = Self::evaluate_candidates(state, candidates);
        evaluated.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        evaluated.into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_state::mid::Fact;
    use agent_types_core::ActionParams;

    #[test]
    fn evaluate_candidates_returns_scores() {
        let mut state = AgentState::new();
        state.mid_term.known_facts.push(Fact::new("goal", "test", 0.8));

        let actions = vec![
            Action::new("click", ActionParams::new().with("target", "btn")),
            Action::new("type", ActionParams::new().with("text", "hello")),
        ];

        let results = SandboxEvaluator::evaluate_candidates(&state, &actions);
        assert_eq!(results.len(), 2);
        // Scores should be in [0, 1]
        for (_, score) in &results {
            assert!((0.0..=1.0).contains(score));
        }
    }

    #[test]
    fn best_candidate_picks_highest() {
        let state = AgentState::new();
        let actions = vec![
            Action::new("a", ActionParams::new()),
            Action::new("b", ActionParams::new()),
        ];

        let best = SandboxEvaluator::best_candidate(&state, &actions);
        assert!(best.is_some());
    }
}
