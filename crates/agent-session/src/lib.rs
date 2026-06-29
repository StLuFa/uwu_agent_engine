//! # agent-session
//!
//! 对话域 —— 持有 Agent 五维 + MVCC 并发 + 能力注册表 + 编排 process_turn 主循环。
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

use agent_character::Character;
use agent_mesh::AgentMesh;
use agent_metacognition::{Metacognition, MetaAction};
use agent_persona::Persona;
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

/// 能力注册表 —— 运行时动态注册 Agent 能力域
pub struct CapabilityRegistry {
    // 能力域将在阶段 3 中通过 trait object 注册
    // perceivers: Vec<Box<dyn Perceiver>>,
    // reasoners: Vec<Box<dyn Reasoner>>,
    // executors: Vec<Box<dyn Executor>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Session —— 用户对话会话，持有五维 + 能力注册表
pub struct Session {
    pub session_id: SessionId,
    pub user_id: String,
    pub agent_id: AgentId,

    // 五维
    pub reaction: Arc<ReactionLayer>,
    pub state: AgentState,
    pub metacognition: Arc<Metacognition>,
    pub persona: Persona,
    pub character: Arc<Character>,

    // 事件网格
    pub mesh: Arc<AgentMesh>,

    // 能力注册
    pub capability_registry: CapabilityRegistry,

    // 对话管理
    pub history: ConversationHistory,
    pub intent_tracker: IntentTracker,

    // 元数据
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

impl Session {
    /// 处理一个对话轮次 —— 五段式主循环
    pub async fn process_turn(&mut self, raw_input: &str) -> TurnResult {
        let input = self.enrich_input(raw_input);

        // 1. Reaction 拦截
        if let Reaction::Hit(action) = self.reaction.intercept(&self.state).await {
            return self.execute_reaction(action).await;
        }

        // 2. FlowGraph: Perception → Memory → Reasoning → Execution
        //    (阶段 3 实现)
        let decision = self.run_flowgraph(&input).await;

        // 3. Metacognition 评估
        let assessment = self.metacognition.evaluate(&self.state, &decision).await;

        // 4. MetaAction 分支处理
        match assessment.suggested_action {
            MetaAction::Proceed => {}
            MetaAction::RetryDecision => { /* TODO: 回滚 State，重新推理 */ }
            MetaAction::RequestClarification => { /* TODO: 暂停，向用户提问 */ }
            MetaAction::SwitchStrategy => { /* TODO: 切换推理模式 */ }
            MetaAction::DelegateToHuman => { /* TODO: 升级 */ }
            MetaAction::AbortOnBudget => { /* TODO: 预算耗尽，终止 */ }
        }

        // 5. 执行 + 校准
        let result = self.execute_and_update(decision).await;

        // 6. Metacognition 在线校准
        //    (阶段 3 实现)
        // self.metacognition.calibrate_with_outcome(&mut self.state, &result.actual_state);

        self.last_active_at = Utc::now();
        result.output
    }

    fn enrich_input(&self, raw: &str) -> EnrichedInput {
        EnrichedInput {
            raw: raw.to_string(),
            persona_context: self.persona.to_context_injection(),
            character_context: self.character.to_context_injection(),
        }
    }

    async fn execute_reaction(&self, action: agent_types_core::Action) -> TurnResult {
        // Reaction 命中：直接执行，不经过 FlowGraph/Metacognition
        TurnResult {
            output: TurnOutput {
                content: format!("reaction: {:?}", action),
                tokens_used: 0,
            },
        }
    }

    async fn run_flowgraph(&self, input: &EnrichedInput) -> agent_reasoning::Decision {
        // TODO: 阶段 3 实现 —— 通过 FlowGraph + visual_script VM 执行
        agent_reasoning::Decision {
            actions: vec![],
            scores: vec![],
            reasoning: "not yet implemented".into(),
        }
    }

    async fn execute_and_update(&mut self, decision: agent_reasoning::Decision) -> ExecutionOutcome {
        // TODO: 阶段 3+ 实现 —— Guard 检查 + 执行 + 修正 State
        ExecutionOutcome {
            output: TurnOutput {
                content: decision.reasoning,
                tokens_used: 0,
            },
        }
    }

    /// 生成快照供 Sidecar 消费
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.session_id,
            state_snapshot: self.state.snapshot(),
            persona_snapshot: self.persona.snapshot(),
            taken_at: Utc::now(),
        }
    }
}

/// 丰富后的输入（含 Persona + Character 上下文注入）
#[derive(Debug, Clone)]
pub struct EnrichedInput {
    pub raw: String,
    pub persona_context: agent_persona::PersonaContext,
    pub character_context: agent_character::CharacterContext,
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

/// 执行结果
#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    pub output: TurnOutput,
}
