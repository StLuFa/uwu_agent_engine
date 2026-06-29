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
use agent_metacognition::{Metacognition, MetaAction};
use agent_persona::{Persona, PersonaContext};
use agent_reaction::{Reaction, ReactionLayer};
use agent_state::AgentState;
use agent_types_core::AgentId;
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

/// Session —— 用户对话会话，持有五维 + 对话管理
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

    // 对话管理
    pub history: ConversationHistory,
    pub intent_tracker: IntentTracker,
    pub turn_count: u64,

    // 元数据
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

impl Session {
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
        let decision = self.run_flowgraph(&input);

        // 3. Metacognition 评估
        let assessment = self
            .metacognition
            .evaluate(&self.state, &decision.command)
            .await;

        // 4. MetaAction 分支处理
        let meta_action_str = format!("{:?}", assessment.suggested_action);
        match assessment.suggested_action {
            MetaAction::Proceed => {}
            MetaAction::RetryDecision => {
                // Rollback state and re-reason
                let checkpoint = self.state.checkpoint();
                let retry_decision = self.run_flowgraph(&input);
                self.state = AgentState::rollback(&checkpoint);
                let output = self.execute_and_update(retry_decision);
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
                let degraded_decision = self.run_flowgraph_degraded(&input);
                let output = self.execute_and_update(degraded_decision);
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
        let result = self.execute_and_update(decision);

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

    fn run_flowgraph(&self, input: &EnrichedInput) -> agent_types_core::Action {
        // Mock: simulate P→M→R→E pipeline
        let _ = input;
        agent_types_core::Action::new(
            "respond",
            agent_types_core::ActionParams::new().with("text", "processed"),
        )
    }

    fn run_flowgraph_degraded(&self, input: &EnrichedInput) -> agent_types_core::Action {
        let _ = input;
        agent_types_core::Action::new(
            "respond_degraded",
            agent_types_core::ActionParams::new().with("text", "degraded mode"),
        )
    }

    fn execute_and_update(&mut self, action: agent_types_core::Action) -> TurnResult {
        self.state.apply_action(&action);
        TurnResult {
            output: TurnOutput {
                content: format!("executed: {}", action.command),
                tokens_used: 0,
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
    use agent_metacognition::{CalibrationModel, CalibrationResult, Metacognition};
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
            history: ConversationHistory::new(),
            intent_tracker: IntentTracker::new(),
            turn_count: 0,
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
}
