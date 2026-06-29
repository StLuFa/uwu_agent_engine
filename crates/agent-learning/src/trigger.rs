//! LearnCondition trait + LearnDecision + LearnTrigger

use crate::{Episode, SkillTarget};
use agent_state::AgentState;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 学习决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearnDecision {
    /// 跳过，不学习
    Skip,
    /// 巩固 Episode 到记忆（不提取 Skill）
    ConsolidateEpisode,
    /// 提取 Skill
    ExtractSkill {
        skill_name: String,
        target: SkillTarget,
        confidence: f32,
    },
    /// 更新偏好（调整 risk_tolerance / uncertainty_strategy）
    UpdatePreference {
        field: String,
        old_value: String,
        new_value: String,
    },
}

/// 学习触发条件 trait
#[async_trait]
pub trait LearnCondition: Send + Sync {
    /// 检查是否应触发学习
    async fn should_learn(
        &self,
        episode: &Episode,
        state: &AgentState,
    ) -> LearnDecision;
}

/// LearnTrigger —— 持有条件列表，顺序评估
pub struct LearnTrigger {
    conditions: Vec<Box<dyn LearnCondition>>,
}

impl LearnTrigger {
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    pub fn with_condition(mut self, condition: Box<dyn LearnCondition>) -> Self {
        self.conditions.push(condition);
        self
    }

    /// 评估所有条件，返回第一个非 Skip 的决策
    pub async fn evaluate(
        &self,
        episode: &Episode,
        state: &AgentState,
    ) -> LearnDecision {
        for cond in &self.conditions {
            let decision = cond.should_learn(episode, state).await;
            if !matches!(decision, LearnDecision::Skip) {
                return decision;
            }
        }
        LearnDecision::Skip
    }

    pub fn condition_count(&self) -> usize {
        self.conditions.len()
    }
}

impl Default for LearnTrigger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysSkip;
    #[async_trait]
    impl LearnCondition for AlwaysSkip {
        async fn should_learn(&self, _: &Episode, _: &AgentState) -> LearnDecision {
            LearnDecision::Skip
        }
    }

    struct AlwaysExtract;
    #[async_trait]
    impl LearnCondition for AlwaysExtract {
        async fn should_learn(&self, _: &Episode, _: &AgentState) -> LearnDecision {
            LearnDecision::ExtractSkill {
                skill_name: "test".into(),
                target: SkillTarget::LocalCode {
                    crate_name: "test".into(),
                },
                confidence: 0.9,
            }
        }
    }

    #[tokio::test]
    async fn first_condition_wins() {
        let trigger = LearnTrigger::new()
            .with_condition(Box::new(AlwaysExtract))
            .with_condition(Box::new(AlwaysSkip));

        let episode = Episode {
            episode_id: "e1".into(),
            session_id: "s1".into(),
            task_id: None,
            state_before: None,
            state_after: None,
            actions_taken: vec![],
            outcome: crate::EpisodeOutcome::Success { confidence: 0.9 },
            timestamp: chrono::Utc::now(),
        };
        let state = AgentState::new();

        let decision = trigger.evaluate(&episode, &state).await;
        assert!(matches!(decision, LearnDecision::ExtractSkill { .. }));
    }

    #[tokio::test]
    async fn all_skip_returns_skip() {
        let trigger = LearnTrigger::new()
            .with_condition(Box::new(AlwaysSkip));

        let episode = Episode {
            episode_id: "e1".into(),
            session_id: "s1".into(),
            task_id: None,
            state_before: None,
            state_after: None,
            actions_taken: vec![],
            outcome: crate::EpisodeOutcome::Success { confidence: 0.9 },
            timestamp: chrono::Utc::now(),
        };
        let state = AgentState::new();

        let decision = trigger.evaluate(&episode, &state).await;
        assert!(matches!(decision, LearnDecision::Skip));
    }
}
