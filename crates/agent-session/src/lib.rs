//! # agent-session
//!
//! 对话域 —— 持有 Agent 五维 + 编排 process_turn 主循环。
//!
//! Session 是整个 Agent 引擎的顶层入口：一个用户会话 = 一个 Session 实例。

mod history;
mod intent;
mod snapshot;
mod turn;

pub use history::ConversationHistory;
pub use intent::IntentTracker;
pub use snapshot::SessionSnapshot;
pub use turn::ConversationTurn;

use agent_character::{Character, CharacterContext};
use agent_execution::ActionExecutor;
use agent_memory::{Episode, MemoryFacade};
use agent_metacognition::{Metacognition, MetaAction};
use agent_perception::PerceptionPipeline;
use agent_persona::{Persona, PersonaContext};
use agent_reaction::{Reaction, ReactionLayer};
use agent_reasoning::Reasoner;
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams, AgentId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Session ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Session —— 用户对话会话，持有五维 + 对话管理 + P→M→R→E 管道
pub struct Session {
    pub session_id: SessionId,
    pub user_id: String,
    pub agent_id: AgentId,

    // 五维
    pub reaction: Arc<ReactionLayer>,
    pub state: AgentState,
    pub metacognition: Metacognition,
    pub persona: Persona,
    pub character: Arc<Character>,

    // P→M→R→E 管道
    pub perception: PerceptionPipeline,
    pub memory: MemoryFacade,
    pub reasoner: Box<dyn Reasoner>,
    pub executor: ActionExecutor,

    // 对话管理
    pub history: ConversationHistory,
    pub intent_tracker: IntentTracker,
    pub turn_count: u64,
    pub checkpoints: Vec<agent_state::StateCheckpoint>,

    // 元数据
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

impl Session {
    /// 在执行外部副作用前自动 checkpoint
    pub fn auto_checkpoint(&mut self) {
        self.checkpoints.push(self.state.checkpoint());
    }

    /// 处理一个对话轮次 —— 五段式主循环
    pub async fn process_turn(&mut self, raw_input: &str) -> TurnResult {
        self.turn_count += 1;
        let input = self.enrich_input(raw_input);

        // 1. Reaction 拦截
        if let Reaction::Hit(action) = self.reaction.intercept(&self.state).await {
            let output = self.execute_reaction(action);
            self.record_turn(raw_input, &output, 1.0, "Hit");
            return output;
        }

        // 2. FlowGraph: Perception → Memory → Reasoning → Execution
        let decision = self.run_flowgraph(&input).await;

        // 3. Metacognition 评估
        let assessment = self
            .metacognition
            .evaluate(&self.state, &decision.command)
            .await;

        // 4. MetaAction 分支处理
        let meta_action_str = format!("{:?}", assessment.suggested_action);
        self.auto_checkpoint(); // checkpoint before potentially state-changing operations
        match assessment.suggested_action {
            MetaAction::Proceed => {}
            MetaAction::RetryDecision => {
                // Rollback state and re-reason
                let checkpoint = self.state.checkpoint();
                let retry_decision = self.run_flowgraph(&input).await;
                self.state = AgentState::rollback(&checkpoint);
                let output = self.execute_and_update(retry_decision, raw_input).await;
                self.record_turn(raw_input, &output, 0.0, &meta_action_str);
                return output;
            }
            MetaAction::RequestClarification => {
                let output = TurnResult {
                    output: TurnOutput {
                        content: "I need more information. Could you clarify?".into(),
                        tokens_used: 0,
                    },
                };
                self.record_turn(raw_input, &output, 0.0, &meta_action_str);
                return output;
            }
            MetaAction::SwitchStrategy => {
                // Switch reasoning strategy (degraded mode)
                let degraded_decision = self.run_flowgraph_degraded(&input).await;
                let output = self.execute_and_update(degraded_decision, raw_input).await;
                self.record_turn(raw_input, &output, 0.0, &meta_action_str);
                return output;
            }
            MetaAction::DelegateToHuman => {
                let output = TurnResult {
                    output: TurnOutput {
                        content: "Escalating to human operator...".into(),
                        tokens_used: 0,
                    },
                };
                self.record_turn(raw_input, &output, 0.0, &meta_action_str);
                return output;
            }
            MetaAction::AbortOnBudget => {
                let output = TurnResult {
                    output: TurnOutput {
                        content: "Budget exhausted. Task terminated.".into(),
                        tokens_used: 0,
                    },
                };
                self.record_turn(raw_input, &output, 0.0, &meta_action_str);
                return output;
            }
        }

        // 5. 执行 + 收集结果
        let result = self.execute_and_update(decision, raw_input).await;

        // 6. Metacognition 在线校准
        let current_state = self.state.clone();
        self.metacognition.calibrate_with_outcome(
            &mut self.state,
            &current_state,
            &assessment.calibration,
            assessment.meta_score,
        );

        self.last_active_at = Utc::now();
        self.record_turn(raw_input, &result, assessment.meta_score, &meta_action_str);
        result
    }

    fn enrich_input(&self, raw: &str) -> EnrichedInput {
        EnrichedInput {
            raw: raw.to_string(),
            persona_context: self.persona.to_context_injection(),
            character_context: self.character.to_context_injection(),
        }
    }

    fn execute_reaction(&self, action: agent_types_core::Action) -> TurnResult {
        TurnResult {
            output: TurnOutput {
                content: format!("reaction: {}", action.command),
                tokens_used: 0,
            },
        }
    }

    /// 真正的 P→M→R→E 管道：
    /// Perception 解析输入 → Memory 检索相关记忆 → Reasoning 生成 Action
    async fn run_flowgraph(&mut self, input: &EnrichedInput) -> Action {
        // 1. Perception: 原始文本 → ContextDescriptor
        let ctx_desc = self.perception.run(&input.raw).await;

        // 2. Memory: 用上下文描述检索相关记忆
        let memories = self.memory.retrieve(&ctx_desc.description);
        let context_str = memories
            .items
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // 3. Reasoning: 基于 State + 目标 + 上下文生成 Action
        let decision = self
            .reasoner
            .reason(
                &self.state,
                "respond to user",
                Some(&context_str),
            )
            .await;

        // 4. 返回最佳动作（fallback: 空响应）
        decision.best_action().cloned().unwrap_or_else(|| {
            Action::new(
                "respond",
                ActionParams::new().with("text", "no action generated"),
            )
        })
    }

    /// 降级推理模式（SwitchStrategy 分支使用）
    async fn run_flowgraph_degraded(&mut self, input: &EnrichedInput) -> Action {
        let ctx_desc = self.perception.run(&input.raw).await;
        Action::new(
            "respond_degraded",
            ActionParams::new().with("text", ctx_desc.description),
        )
    }

    /// 真正的执行 + 记忆持久化：
    /// ActionExecutor 执行动作 → apply_action 更新 State → 持久化到 Memory
    async fn execute_and_update(
        &mut self,
        action: Action,
        raw_input: &str,
    ) -> TurnResult {
        let result = self.executor.execute_action(&action, &self.state).await;
        self.state.apply_action(&action);

        // 持久化 State 快照到记忆
        let snap_json =
            serde_json::to_string(&self.state.snapshot()).unwrap_or_default();
        self.memory
            .persist_state(self.agent_id.to_string(), &snap_json);

        // 巩固 Episode：记录完整的交互回合
        let episode = Episode::new(
            self.agent_id.to_string(),
            "respond to user",
            &result.output,
            result.success,
        )
        .with_action(action.command.clone())
        .with_observation(raw_input);
        self.memory.consolidate(&episode);

        TurnResult {
            output: TurnOutput {
                content: result.output,
                tokens_used: result.tokens_used,
            },
        }
    }

    fn record_turn(
        &mut self,
        user_input: &str,
        result: &TurnResult,
        _meta_score: f32,
        meta_action: &str,
    ) {
        let turn = ConversationTurn::new(
            self.turn_count,
            user_input,
            &result.output.content,
            result.output.tokens_used,
            meta_action,
        );
        // Intent tracking
        self.intent_tracker
            .update(self.intent_tracker.infer(user_input));
        self.history.push(turn);
    }

    /// 生成快照供 Sidecar 消费
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot::new(
            self.session_id.0.to_string(),
            self.state.snapshot(),
            self.persona.version,
            self.history.len(),
            self.history.total_tokens(),
        )
    }
}

/// 丰富后的输入（含 Persona + Character 上下文注入）
#[derive(Debug, Clone)]
pub struct EnrichedInput {
    pub raw: String,
    pub persona_context: PersonaContext,
    pub character_context: CharacterContext,
}

/// 轮次输出
#[derive(Debug, Clone)]
pub struct TurnOutput {
    pub content: String,
    pub tokens_used: u64,
}

/// 轮次结果
#[derive(Debug, Clone)]
pub struct TurnResult {
    pub output: TurnOutput,
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use agent_character::{Character, Preferences};
    use agent_execution::ActionExecutor;
    use agent_memory::MemoryFacade;
    use agent_metacognition::{CalibrationModel, CalibrationResult, Metacognition};
    use agent_perception::PerceptionPipeline;
    use agent_persona::{Identity, PersonaHistory, RelationshipGraph};
    use agent_reaction::ReactionLayer;
    use agent_state::AgentState;
    use async_trait::async_trait;
    use chrono::Duration;

    struct MockCalibrator;
    #[async_trait]
    impl CalibrationModel for MockCalibrator {
        async fn calibrate(&self, _state: &AgentState, _text: &str) -> CalibrationResult {
            CalibrationResult {
                raw_confidence: 0.9,
                calibrated_confidence: 0.9,
                should_retry: false,
                reasoning: "ok".into(),
            }
        }
    }

    /// 简单的测试用 Reasoner：根据上下文生成响应动作
    struct SimpleReasoner;
    #[async_trait]
    impl Reasoner for SimpleReasoner {
        async fn reason(
            &self,
            _state: &AgentState,
            _goal: &str,
            context: Option<&str>,
        ) -> agent_reasoning::Decision {
            let text = context.unwrap_or("ok");
            agent_reasoning::Decision::single(
                Action::new(
                    "respond",
                    ActionParams::new().with("text", text),
                ),
                0.8,
                format!("responding to: {text}"),
            )
        }
    }

    fn make_session() -> Session {
        let reaction = Arc::new(ReactionLayer::builder().build());
        let metacog = Metacognition::new(
            Box::new(MockCalibrator),
            10_000,
            Duration::seconds(120),
            5,
        );
        Session {
            session_id: SessionId::new(),
            user_id: "user-1".into(),
            agent_id: AgentId::new(),
            reaction,
            state: AgentState::new(),
            metacognition: metacog,
            persona: Persona {
                version: 0,
                identity: Identity::default(),
                relationships: RelationshipGraph::new(),
                history: PersonaHistory::new(),
            },
            character: Arc::new(Character {
                core_values: vec![],
                preferences: Preferences::default(),
            }),
            perception: PerceptionPipeline::new(),
            memory: MemoryFacade::new(16),
            reasoner: Box::new(SimpleReasoner),
            executor: ActionExecutor::new(),
            history: ConversationHistory::new(),
            intent_tracker: IntentTracker::new(),
            turn_count: 0,
            checkpoints: Vec::new(),
            created_at: Utc::now(),
            last_active_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn process_turn_proceed() {
        let mut session = make_session();
        let result = session.process_turn("hello").await;
        assert!(!result.output.content.is_empty());
        assert!(session.turn_count > 0);
    }

    #[tokio::test]
    async fn process_turn_updates_history() {
        let mut session = make_session();
        session.process_turn("search for docs").await;
        assert_eq!(session.history.len(), 1);
    }

    #[tokio::test]
    async fn process_turn_updates_intent() {
        let mut session = make_session();
        session.process_turn("find the file").await;
        assert_eq!(
            session.intent_tracker.current_intent,
            Some("search".into())
        );
    }

    #[tokio::test]
    async fn snapshot_contains_state() {
        let session = make_session();
        let snap = session.snapshot();
        assert_eq!(snap.turn_count, 0);
    }

    #[tokio::test]
    async fn checkpoint_created_during_process_turn() {
        let mut session = make_session();
        session.process_turn("test input").await;
        assert!(!session.checkpoints.is_empty());
    }

    #[tokio::test]
    async fn integration_full_session_with_all_dimensions() {
        // Build a complete session with all 5 dimensions + real P→M→R→E pipeline
        let reaction = Arc::new(ReactionLayer::builder().build());
        let metacog = Metacognition::new(
            Box::new(MockCalibrator),
            10_000,
            Duration::seconds(120),
            5,
        );

        let mut session = Session {
            session_id: SessionId::new(),
            user_id: "user-1".into(),
            agent_id: AgentId::new(),
            reaction,
            state: AgentState::new(),
            metacognition: metacog,
            persona: Persona {
                version: 0,
                identity: Identity::new("Alice", "assistant")
                    .with_expertise(vec!["search".into(), "code".into()]),
                relationships: RelationshipGraph::new(),
                history: PersonaHistory::new(),
            },
            character: Arc::new(Character {
                core_values: vec![],
                preferences: Preferences::default(),
            }),
            perception: PerceptionPipeline::new(),
            memory: MemoryFacade::new(16),
            reasoner: Box::new(SimpleReasoner),
            executor: ActionExecutor::new(),
            history: ConversationHistory::new(),
            intent_tracker: IntentTracker::new(),
            turn_count: 0,
            checkpoints: Vec::new(),
            created_at: Utc::now(),
            last_active_at: Utc::now(),
        };

        // Process multiple turns
        let r1 = session.process_turn("search for rust docs").await;
        assert!(!r1.output.content.is_empty());

        let r2 = session.process_turn("click result #3").await;
        assert!(!r2.output.content.is_empty());

        // Verify full pipeline executed
        assert_eq!(session.turn_count, 2);
        assert_eq!(session.history.len(), 2);
        assert!(!session.checkpoints.is_empty());
        assert!(session.state.short_term.last_action.is_some());

        // Verify persona context is available
        let snap = session.snapshot();
        assert_eq!(snap.persona_version, 0);
        assert_eq!(snap.turn_count, 2);
    }
}
