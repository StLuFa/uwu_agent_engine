//! LearnCondition implementations

use crate::trigger::LearnDecision;
use crate::{Episode, EpisodeOutcome, SkillTarget};
use agent_state::AgentState;
use async_trait::async_trait;

use super::trigger::LearnCondition;

/// 显著错误条件 —— 预测误差超过阈值 → ExtractSkill
pub struct SignificantErrorCondition {
    pub error_threshold: f32,
}

impl SignificantErrorCondition {
    pub fn new(threshold: f32) -> Self {
        Self {
            error_threshold: threshold,
        }
    }
}

#[async_trait]
impl LearnCondition for SignificantErrorCondition {
    async fn should_learn(&self, episode: &Episode, state: &AgentState) -> LearnDecision {
        let pred_error = state.long_term.accumulated_pred_error;

        if pred_error > self.error_threshold {
            return LearnDecision::ExtractSkill {
                skill_name: format!("fix-error-{}", episode.episode_id),
                target: SkillTarget::LocalCode {
                    crate_name: "agent-reaction".into(),
                },
                confidence: (pred_error * 0.8).clamp(0.0, 1.0),
            };
        }
        LearnDecision::Skip
    }
}

/// 新模式条件 —— Episode 成功 + 高置信度 → ExtractSkill
pub struct NewPatternCondition {
    pub min_confidence: f32,
}

impl NewPatternCondition {
    pub fn new(min_confidence: f32) -> Self {
        Self { min_confidence }
    }
}

#[async_trait]
impl LearnCondition for NewPatternCondition {
    async fn should_learn(&self, episode: &Episode, _state: &AgentState) -> LearnDecision {
        if let EpisodeOutcome::Success { confidence } = &episode.outcome {
            if *confidence >= self.min_confidence && !episode.actions_taken.is_empty() {
                return LearnDecision::ExtractSkill {
                    skill_name: format!("pattern-{}", episode.episode_id),
                    target: SkillTarget::LocalCode {
                        crate_name: "agent-reaction".into(),
                    },
                    confidence: *confidence,
                };
            }
        }
        LearnDecision::Skip
    }
}

/// 用户确认条件 —— Success + 用户显式确认 → ConsolidateEpisode
pub struct UserConfirmedCondition;

#[async_trait]
impl LearnCondition for UserConfirmedCondition {
    async fn should_learn(&self, episode: &Episode, _state: &AgentState) -> LearnDecision {
        if matches!(episode.outcome, EpisodeOutcome::Success { .. }) {
            return LearnDecision::ConsolidateEpisode;
        }
        LearnDecision::Skip
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_episode(outcome: EpisodeOutcome, actions: Vec<String>) -> Episode {
        Episode {
            episode_id: "ep-1".into(),
            session_id: "s-1".into(),
            task_id: None,
            state_before: None,
            state_after: None,
            actions_taken: actions,
            outcome,
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn significant_error_triggers_on_high_pred_error() {
        let mut state = AgentState::new();
        state.long_term.accumulated_pred_error = 0.5;
        let cond = SignificantErrorCondition::new(0.3);
        let episode = make_episode(
            EpisodeOutcome::Failure {
                error: "timeout".into(),
            },
            vec!["click".into()],
        );
        let decision = cond.should_learn(&episode, &state).await;
        assert!(matches!(decision, LearnDecision::ExtractSkill { .. }));
    }

    #[tokio::test]
    async fn significant_error_skips_on_low_pred_error() {
        let mut state = AgentState::new();
        state.long_term.accumulated_pred_error = 0.1;
        let cond = SignificantErrorCondition::new(0.3);
        let episode = make_episode(
            EpisodeOutcome::Failure {
                error: "timeout".into(),
            },
            vec![],
        );
        let decision = cond.should_learn(&episode, &state).await;
        assert!(matches!(decision, LearnDecision::Skip));
    }

    #[tokio::test]
    async fn new_pattern_extracts_on_high_confidence() {
        let cond = NewPatternCondition::new(0.7);
        let episode = make_episode(
            EpisodeOutcome::Success { confidence: 0.9 },
            vec!["search".into(), "click".into()],
        );
        let state = AgentState::new();
        let decision = cond.should_learn(&episode, &state).await;
        assert!(matches!(decision, LearnDecision::ExtractSkill { .. }));
    }

    #[tokio::test]
    async fn user_confirmed_consolidates() {
        let cond = UserConfirmedCondition;
        let episode = make_episode(
            EpisodeOutcome::Success { confidence: 0.8 },
            vec!["click".into()],
        );
        let state = AgentState::new();
        let decision = cond.should_learn(&episode, &state).await;
        assert!(matches!(decision, LearnDecision::ConsolidateEpisode));
    }
}
