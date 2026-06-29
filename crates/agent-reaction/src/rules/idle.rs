//! IdleTimeoutRule —— 检测连续无进展，重新评估目标

use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

use super::super::ReactionRule;

/// 无进展判定阈值
const FAILURE_LOOP_SUCCESS_THRESHOLD: f32 = 0.3;
const FAILURE_LOOP_CONSECUTIVE_STEPS: u32 = 5;

/// 检测长期无进展 → 重新评估目标
pub struct IdleTimeoutRule;

#[async_trait]
impl ReactionRule for IdleTimeoutRule {
    fn matches(&self, state: &AgentState) -> bool {
        // 1. 检测失败循环（由 InteractionPattern 提供）
        if let Some(ref pattern) = state.mid_term.recent_pattern {
            if pattern.is_failure_loop(FAILURE_LOOP_SUCCESS_THRESHOLD, FAILURE_LOOP_CONSECUTIVE_STEPS)
            {
                return true;
            }
        }

        // 2. 上一动作为空 + 有历史动作（说明停滞了）
        if state.short_term.last_action.is_none()
            && !state.mid_term.action_history.is_empty()
        {
            return true;
        }

        false
    }

    async fn react(&self, _state: &AgentState) -> Action {
        Action::new("re_evaluate_goal", ActionParams::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_state::mid::InteractionPattern;
    use agent_state::AgentState;
    use agent_types_core::{Action, ActionParams};

    #[test]
    fn matches_failure_loop() {
        let mut state = AgentState::new();
        state.mid_term.recent_pattern = Some(InteractionPattern {
            recent_success_rate: 0.2,
            detected_pattern: Some("loop_detected".into()),
            pattern_since_step: 6,
        });
        assert!(IdleTimeoutRule.matches(&state));
    }

    #[test]
    fn matches_stalled_no_last_action() {
        let mut state = AgentState::new();
        // Has history but no last action → stalled
        state.mid_term.action_history.push(
            agent_state::mid::ActionRecord::new(
                Action::new("click", ActionParams::new()),
                agent_types_core::ActionStatus::Committed,
            ),
        );
        assert!(IdleTimeoutRule.matches(&state));
    }

    #[test]
    fn no_match_active_state() {
        let mut state = AgentState::new();
        state.short_term.last_action = Some(Action::new("click", ActionParams::new()));
        assert!(!IdleTimeoutRule.matches(&state));
    }

    #[test]
    fn no_match_empty_state() {
        let state = AgentState::new();
        assert!(!IdleTimeoutRule.matches(&state));
    }
}
