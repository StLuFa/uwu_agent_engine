//! Consolidator —— 独立巩固进程。
//!
//! 消费 Episode → LearnTrigger 评估 → Guard 博弈 → Memory 持久化。

use agent_guard::{
    rules::{McpWriteAllowlistRule, TokenBudgetRule},
    AgentContext, GuardLayer,
};
use agent_learning::{
    conditions::{NewPatternCondition, SignificantErrorCondition, UserConfirmedCondition},
    LearnDecision, LearnTrigger, SkillTarget,
};
use agent_memory::MemoryFacade;
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use std::sync::Arc;

/// Consolidator 配置
struct Config {
    max_tokens: u64,
    max_retries: u32,
    egress_allowlist: Vec<String>,
    poll_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tokens: 10_000,
            max_retries: 5,
            egress_allowlist: vec!["safe-server".into()],
            poll_interval_ms: 1000,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[consolidator] starting...");

    let cfg = Config::default();

    // 1. Guard layer
    let guard = Arc::new(
        GuardLayer::builder()
            .add_budget_rule(TokenBudgetRule)
            .add_egress_rule(McpWriteAllowlistRule {
                allowed_targets: cfg.egress_allowlist.clone(),
            })
            .build(),
    );

    // 2. Learn trigger
    let learn_trigger = LearnTrigger::new()
        .with_condition(Box::new(SignificantErrorCondition::new(0.3)))
        .with_condition(Box::new(NewPatternCondition::new(0.7)))
        .with_condition(Box::new(UserConfirmedCondition));

    // 3. Memory
    let mut memory = MemoryFacade::new(16);

    // 4. Context
    let ctx = AgentContext {
        session_id: "consolidator".into(),
        agent_id: "consolidator".into(),
        tokens_used: 0,
        max_tokens: cfg.max_tokens,
        retries: 0,
        max_retries: cfg.max_retries,
    };

    // 5. Main loop: poll for episodes
    let mut episode_count = 0u64;
    loop {
        // Simulate receiving an episode (production: from agent-mesh consolidation channel)
        let episode = mock_episode(episode_count);
        episode_count += 1;

        let state = AgentState::new();

        // Evaluate learning trigger
        let decision = learn_trigger.evaluate(&episode, &state).await;

        match decision {
            LearnDecision::ExtractSkill {
                ref skill_name,
                ref target,
                confidence,
            } => {
                // Guard egress check for McpRemote
                if let SkillTarget::McpRemote { endpoint, .. } = target {
                    if guard.check_egress(endpoint).await.is_err() {
                        println!("[consolidator] egress blocked: {endpoint}");
                        continue;
                    }
                }

                // Guard enforce: verify action safety
                let action = Action::new(
                    format!("extract_{skill_name}"),
                    ActionParams::new().with("confidence", confidence),
                );
                if guard.enforce(&[action], &ctx).await.is_err() {
                    println!("[consolidator] guard blocked: {skill_name}");
                    continue;
                }

                memory.consolidate(&agent_memory::Episode::new(
                    "consolidator",
                    &episode.actions_taken.join(","),
                    format!("extracted: {skill_name}"),
                    true,
                ));
                println!("[consolidator] extracted skill: {skill_name} (confidence: {confidence})");
            }

            LearnDecision::ConsolidateEpisode => {
                memory.consolidate(&agent_memory::Episode::new(
                    "consolidator",
                    &episode.actions_taken.join(","),
                    "consolidated",
                    true,
                ));
                println!("[consolidator] consolidated episode {}", episode.episode_id);
            }

            LearnDecision::Skip => {
                // Nothing to learn
            }

            LearnDecision::UpdatePreference { field, .. } => {
                println!("[consolidator] preference update: {field}");
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(cfg.poll_interval_ms)).await;

        if episode_count >= 5 {
            println!("[consolidator] processed {} episodes, shutting down", episode_count);
            break;
        }
    }

    Ok(())
}

fn mock_episode(n: u64) -> agent_learning::Episode {
    agent_learning::Episode {
        episode_id: format!("ep-{n}"),
        session_id: "s-1".into(),
        task_id: None,
        state_before: None,
        state_after: None,
        actions_taken: vec!["search".into(), "click".into()],
        outcome: agent_learning::EpisodeOutcome::Success { confidence: 0.9 },
        timestamp: chrono::Utc::now(),
    }
}
