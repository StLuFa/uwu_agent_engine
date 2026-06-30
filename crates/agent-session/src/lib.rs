//! # agent-session
//!
//! 对话域 —— 持有 Agent 五维 + 编排 process_turn 主循环。
//!
//! Session 是整个 Agent 引擎的顶层入口：一个用户会话 = 一个 Session 实例。
//!
//! ## 已接入的外部系统
//!
//! - **agent-mesh**：每轮 turn 发布 StateSnapshot / DecisionMade 事件到事件网格
//! - **agent-core**：使用 FlowGraph::standard() 定义 P→M→R→E 管道拓扑
//! - **agent-task**：支持 create_task / task 状态追踪
//! - **agent-collaboration**：支持 delegate_subtask / 多 Agent 注册

mod history;
mod intent;
mod snapshot;
mod turn;

pub use history::ConversationHistory;
pub use intent::IntentTracker;
pub use snapshot::SessionSnapshot;
pub use turn::ConversationTurn;

use agent_character::{Character, CharacterContext};
use agent_collaboration::{AgentDescriptor, AgentRegistry, Collaboration, DelegationResult};
use agent_core::flow::{FlowGraph, Stage};
use agent_execution::ActionExecutor;
use agent_memory::{Episode, MemoryFacade};
use agent_mesh::AgentMesh;
use agent_mesh::events::decision::DecisionMade;
use agent_mesh::events::state::StateSnapshotEvent;
use agent_mesh::events::task::TaskCreated;
use agent_metacognition::{Metacognition, MetaAction};
use agent_perception::PerceptionPipeline;
use agent_persona::{Persona, PersonaContext};
use agent_reaction::{Reaction, ReactionLayer};
use agent_reasoning::Reasoner;
use agent_state::AgentState;
use agent_task::{Goal, Task, TaskManifest};
use agent_types_core::{Action, ActionParams, AgentId};
use agent_wiki::{MemoryWikiStore, WikiPage, WikiRepo};
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
    pub pipeline_topology: FlowGraph,

    // 事件网格（跨进程 + Sidecar 消费）
    pub event_mesh: Option<Arc<AgentMesh>>,

    // Wiki 知识库
    pub wiki: Option<MemoryWikiStore>,

    // 任务 + 协作
    pub active_tasks: Vec<Task>,
    pub task_manifest: TaskManifest,
    pub agent_registry: AgentRegistry,
    pub collaboration: Option<Collaboration>,

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
    /// Attach an event mesh for cross-process event publishing.
    pub fn with_event_mesh(mut self, mesh: Arc<AgentMesh>) -> Self {
        self.event_mesh = Some(mesh);
        self
    }

    /// Set custom pipeline topology (default: FlowGraph::standard()).
    pub fn with_pipeline(mut self, graph: FlowGraph) -> Self {
        self.pipeline_topology = graph;
        self
    }

    /// Enable wiki knowledge base for this session.
    pub fn with_wiki(mut self, wiki: MemoryWikiStore) -> Self {
        self.wiki = Some(wiki);
        self
    }

    /// Save a decision as a wiki page for future reference.
    pub async fn save_to_wiki(&mut self, title: &str, content: &str, category: &str) {
        if let Some(ref mut wiki) = self.wiki {
            let mut page = WikiPage::new(
                title,
                content,
                category,
                &self.agent_id.to_string(),
            );
            page.publish();
            if let Err(e) = wiki.save(&page).await {
                eprintln!("[session] wiki save failed: {e}");
            }
        }
    }

    /// Search the wiki for relevant knowledge.
    pub async fn search_wiki(&self, query: &str) -> Vec<WikiPage> {
        if let Some(ref wiki) = self.wiki {
            wiki.search(query).await.unwrap_or_default()
        } else {
            vec![]
        }
    }

    /// Enable collaboration with the given agent registry.
    pub fn with_collaboration(mut self, registry: AgentRegistry) -> Self {
        let reg = Arc::new(registry);
        self.collaboration = Some(Collaboration::new(reg.clone()));
        self.agent_registry = Arc::try_unwrap(reg).unwrap_or_else(|r| (*r).clone());
        self
    }

    /// Register a peer agent for future delegation.
    pub fn register_peer_agent(&mut self, desc: AgentDescriptor) {
        self.agent_registry.register(desc);
        let reg = Arc::new(self.agent_registry.clone());
        self.collaboration = Some(Collaboration::new(reg));
    }

    /// Create a task from goal + manifest, ready for DAG-based execution.
    pub fn create_task(&mut self, goal: Goal, manifest: TaskManifest) {
        let task = Task::new(goal, manifest);
        self.publish_event_task_created(&task);
        self.active_tasks.push(task);
    }

    /// Delegate a subtask to the best available peer agent by capability.
    pub fn delegate_subtask(
        &mut self,
        task_description: &str,
        capability: &str,
    ) -> Option<DelegationResult> {
        let collab = self.collaboration.as_mut()?;
        collab.delegate(task_description, self.agent_id.clone(), capability)
    }

    /// Update task progress based on completed DAG nodes.
    pub fn check_task_progress(&mut self) {
        for task in &mut self.active_tasks {
            task.update_progress();
        }
    }

    // ---- event publishing helpers ----

    fn publish_event_state_snapshot(&self) {
        if let Some(ref mesh) = self.event_mesh {
            let snap = self.state.snapshot();
            let snap_json = serde_json::to_string(&snap).unwrap_or_default();
            let version = snap.snapshot_version;
            let event = StateSnapshotEvent::new(
                &self.agent_id.to_string(),
                &snap_json,
                version,
            );
            if let Ok(topic) = uwu_event_mesh::Topic::new("agent.state.snapshot") {
                let _ = mesh.mesh.emit(&topic, serde_json::to_value(&event).unwrap_or_default());
            }
        }
    }

    fn publish_event_decision_made(&self, command: &str, meta_score: f32, meta_action: &str) {
        if let Some(ref mesh) = self.event_mesh {
            let event = DecisionMade::new(
                &self.agent_id.to_string(),
                command,
                meta_score,
                meta_action,
                self.history.total_tokens(),
            );
            if let Ok(topic) = uwu_event_mesh::Topic::new("agent.decision.made") {
                let _ = mesh.mesh.emit(&topic, serde_json::to_value(&event).unwrap_or_default());
            }
        }
    }

    fn publish_event_task_created(&self, task: &Task) {
        if let Some(ref mesh) = self.event_mesh {
            let event = TaskCreated::new(
                task.task_id.to_string(),
                &task.goal.description,
                task.goal.priority,
                &self.agent_id.to_string(),
            );
            if let Ok(topic) = uwu_event_mesh::Topic::new("agent.task.created") {
                let _ = mesh.mesh.emit(&topic, serde_json::to_value(&event).unwrap_or_default());
            }
        }
    }

    // ---- core pipeline methods ----

    /// 在执行外部副作用前自动 checkpoint
    pub fn auto_checkpoint(&mut self) {
        self.checkpoints.push(self.state.checkpoint());
    }

    /// 处理一个对话轮次 —— 六段式主循环（含事件发布）
    pub async fn process_turn(&mut self, raw_input: &str) -> TurnResult {
        self.turn_count += 1;
        let input = self.enrich_input(raw_input);

        // 1. Reaction 拦截
        if let Reaction::Hit(action) = self.reaction.intercept(&self.state).await {
            let output = self.execute_reaction(action);
            self.record_turn(raw_input, &output, 1.0, "Hit");
            self.publish_event_decision_made(&output.output.content, 1.0, "ReactionHit");
            return output;
        }

        // 2. FlowGraph 管道 (topology from agent-core::FlowGraph)
        let decision = self.run_flowgraph(&input).await;

        // 3. Metacognition 评估
        let assessment = self
            .metacognition
            .evaluate(&self.state, &decision.command)
            .await;

        // 4. MetaAction 分支处理
        let meta_action_str = format!("{:?}", assessment.suggested_action);
        self.auto_checkpoint();
        match assessment.suggested_action {
            MetaAction::Proceed => {}
            MetaAction::RetryDecision => {
                let checkpoint = self.state.checkpoint();
                let retry_decision = self.run_flowgraph(&input).await;
                self.state = AgentState::rollback(&checkpoint)
                    .unwrap_or_else(|e| {
                        eprintln!("[session] checkpoint rollback failed: {e}, continuing with current state");
                        self.state.clone()
                    });
                let output = self.execute_and_update(retry_decision, raw_input).await;
                self.record_turn(raw_input, &output, 0.0, &meta_action_str);
                self.publish_event_decision_made(&output.output.content, 0.0, "RetryDecision");
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

        // 7. 发布事件到 mesh
        self.publish_event_state_snapshot();
        self.publish_event_decision_made(
            &result.output.content,
            assessment.meta_score,
            &meta_action_str,
        );

        result
    }

    /// 当前 pipeline 拓扑的阶段列表
    pub fn pipeline_stages(&self) -> &[Stage] {
        &self.pipeline_topology.config.stages
    }

    /// 是否使用高安全模式（含 Validate 验证回边）
    pub fn is_high_security(&self) -> bool {
        self.pipeline_topology.config.validation_loop
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
            pipeline_topology: FlowGraph::standard(),
            event_mesh: None,
            wiki: None,
            active_tasks: Vec::new(),
            task_manifest: TaskManifest::default(),
            agent_registry: AgentRegistry::new(),
            collaboration: None,
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
            pipeline_topology: FlowGraph::standard(),
            event_mesh: None,
            wiki: None,
            active_tasks: Vec::new(),
            task_manifest: TaskManifest::default(),
            agent_registry: AgentRegistry::new(),
            collaboration: None,
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

    // ---- Integration tests: mesh + core + task + collaboration ----

    #[test]
    fn pipeline_topology_defaults_to_standard() {
        let session = make_session();
        let stages = session.pipeline_stages();
        assert_eq!(stages.len(), 4);
        assert!(stages.contains(&Stage::Perception));
        assert!(stages.contains(&Stage::Memory));
        assert!(stages.contains(&Stage::Reasoning));
        assert!(stages.contains(&Stage::Execution));
    }

    #[test]
    fn high_security_pipeline_has_validate() {
        let mut session = make_session();
        session.pipeline_topology = FlowGraph::high_security();
        assert!(session.is_high_security());
        assert!(session.pipeline_stages().contains(&Stage::Validate));
    }

    #[tokio::test]
    async fn process_turn_with_mesh_does_not_crash() {
        let mut session = make_session();
        // Event mesh is None — publishing should be a no-op, not crash.
        let result = session.process_turn("test with mesh none").await;
        assert!(!result.output.content.is_empty());
    }

    #[test]
    fn create_task_adds_to_active_tasks() {
        let mut session = make_session();
        let goal = Goal {
            description: "test task".into(),
            success_criteria: vec!["done".into()],
            priority: 3,
        };
        let manifest = TaskManifest::new("Test Task", "A test task for integration");
        session.create_task(goal, manifest);
        assert_eq!(session.active_tasks.len(), 1);
        assert_eq!(session.active_tasks[0].goal.description, "test task");
    }

    #[test]
    fn register_peer_agent_enables_collaboration() {
        let mut session = make_session();
        let desc = AgentDescriptor::new(
            AgentId::new(),
            "peer-agent",
            "worker",
        ).with_capabilities(vec!["search".into(), "click".into()]);
        session.register_peer_agent(desc);
        // Should have collaboration enabled after registration.
        assert!(session.collaboration.is_some());
    }

    #[test]
    fn delegate_subtask_finds_best_agent() {
        let mut session = make_session();
        let desc = AgentDescriptor::new(AgentId::new(), "searcher", "worker")
            .with_capabilities(vec!["search".into()])
            .with_trust(0.9);
        session.register_peer_agent(desc);

        let result = session.delegate_subtask("find docs about Rust", "search");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.delegation_id.0.is_empty());
    }

    #[test]
    fn check_task_progress_empty_tasks_is_noop() {
        let mut session = make_session();
        session.check_task_progress(); // Should not crash with empty tasks
        assert!(session.active_tasks.is_empty());
    }
}
