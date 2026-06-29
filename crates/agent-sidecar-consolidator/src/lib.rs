//! Consolidator library — 可嵌入的 Episode 巩固引擎。
//!
//! 消费 `agent_learning::Episode` 流 → LearnTrigger 评估 → Guard 检查 → Memory 持久化。
//!
//! ```ignore
//! use agent_sidecar_consolidator::Consolidator;
//! let (tx, rx) = tokio::sync::mpsc::channel(64);
//! let mut consolidator = Consolidator::new();
//! tokio::spawn(async move { consolidator.run(rx).await });
//! tx.send(episode).await.unwrap();
//! ```

use agent_guard::{
    rules::{McpWriteAllowlistRule, TokenBudgetRule},
    AgentContext, GuardLayer,
};
use agent_learning::{
    conditions::{NewPatternCondition, SignificantErrorCondition, UserConfirmedCondition},
    Episode, LearnDecision, LearnTrigger, SkillTarget,
};
use agent_memory::MemoryFacade;
use agent_state::AgentState;
use std::sync::Arc;

/// Consolidator 配置
pub struct Config {
    pub max_tokens: u64,
    pub max_retries: u32,
    pub egress_allowlist: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tokens: 10_000,
            max_retries: 5,
            egress_allowlist: vec!["safe-server".into()],
        }
    }
}

/// 巩固引擎 —— 消费 Episode 流，运行 LearnTrigger + Guard + Memory
pub struct Consolidator {
    cfg: Config,
    guard: Arc<GuardLayer>,
    learn_trigger: LearnTrigger,
    memory: MemoryFacade,
    episode_count: u64,
}

impl Consolidator {
    pub fn new() -> Self {
        let cfg = Config::default();
        let guard = Arc::new(
            GuardLayer::builder()
                .add_budget_rule(TokenBudgetRule)
                .add_egress_rule(McpWriteAllowlistRule {
                    allowed_targets: cfg.egress_allowlist.clone(),
                })
                .build(),
        );
        let learn_trigger = LearnTrigger::new()
            .with_condition(Box::new(SignificantErrorCondition::new(0.3)))
            .with_condition(Box::new(NewPatternCondition::new(0.7)))
            .with_condition(Box::new(UserConfirmedCondition));

        Self {
            cfg,
            guard,
            learn_trigger,
            memory: MemoryFacade::new(16),
            episode_count: 0,
        }
    }

    pub fn with_config(mut self, cfg: Config) -> Self {
        self.cfg = cfg;
        self.guard = Arc::new(
            GuardLayer::builder()
                .add_budget_rule(TokenBudgetRule)
                .add_egress_rule(McpWriteAllowlistRule {
                    allowed_targets: self.cfg.egress_allowlist.clone(),
                })
                .build(),
        );
        self
    }

    /// 处理一个 Episode
    pub async fn process(&mut self, episode: &Episode) {
        self.episode_count += 1;
        let state = AgentState::new();

        let decision = self.learn_trigger.evaluate(episode, &state).await;

        match decision {
            LearnDecision::ExtractSkill {
                ref skill_name,
                ref target,
                confidence,
            } => {
                if let SkillTarget::McpRemote { endpoint, .. } = target {
                    if self.guard.check_egress(endpoint).await.is_err() {
                        println!(
                            "[consolidator] egress blocked: {endpoint}"
                        );
                        return;
                    }
                }

                let action = agent_types_core::Action::new(
                    format!("extract_{skill_name}"),
                    agent_types_core::ActionParams::new()
                        .with("confidence", confidence),
                );

                let ctx = AgentContext {
                    session_id: "consolidator".into(),
                    agent_id: "consolidator".into(),
                    tokens_used: 0,
                    max_tokens: self.cfg.max_tokens,
                    retries: 0,
                    max_retries: self.cfg.max_retries,
                };

                if self.guard.enforce(&[action], &ctx).await.is_err() {
                    println!(
                        "[consolidator] guard blocked: {skill_name}"
                    );
                    return;
                }

                self.memory.consolidate(&agent_memory::Episode::new(
                    "consolidator",
                    &episode.actions_taken.join(","),
                    format!("extracted: {skill_name}"),
                    true,
                ));
                println!(
                    "[consolidator] extracted skill: {skill_name} (conf: {confidence})"
                );
            }

            LearnDecision::ConsolidateEpisode => {
                let outcome_str = format!("{:?}", episode.outcome);
                self.memory.consolidate(&agent_memory::Episode::new(
                    "consolidator",
                    &episode.actions_taken.join(","),
                    &outcome_str,
                    true,
                ));
                println!(
                    "[consolidator] consolidated episode {}",
                    episode.episode_id
                );
            }

            LearnDecision::Skip => {}

            LearnDecision::UpdatePreference { field, .. } => {
                println!("[consolidator] preference update: {field}");
            }
        }
    }

    /// 运行巩固循环：从 channel 消费 Episode 直到 channel 关闭。
    pub async fn run(&mut self, mut rx: tokio::sync::mpsc::Receiver<Episode>) {
        println!("[consolidator] running (channel-based, awaiting episodes)...");
        while let Some(episode) = rx.recv().await {
            self.process(&episode).await;
        }
        println!(
            "[consolidator] channel closed, processed {} episodes",
            self.episode_count
        );
    }

    /// 返回已处理的 episode 数量
    pub fn episode_count(&self) -> u64 {
        self.episode_count
    }
}

impl Default for Consolidator {
    fn default() -> Self {
        Self::new()
    }
}
