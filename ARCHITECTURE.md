# uwu_agent_engine—— 反应式 AI Agent 引擎系统架构

---

## 目录

1. [设计原则](#1-设计原则)
2. [架构全景](#2-架构全景)
3. [Agent 五维](#3-agent-五维)
4. [Session + MVCC 并发模型](#4-session--mvcc-并发模型)
5. [Task —— 任务域](#5-task--任务域)
6. [Collaboration —— 多 Agent 协作域](#6-collaboration--多-agent-协作域)
7. [FlowGraph —— 动态数据流](#7-flowgraph--动态数据流)
8. [agent-mesh —— 事件网格（跨进程安全）](#8-agent-mesh--事件网格跨进程安全)
9. [UnifiedMemory —— 统一记忆](#9-unifiedmemory--统一记忆)
10. [GuardLayer —— 安全守卫](#10-guardlayer--安全守卫)
11. [Sidecar —— 独立进程](#11-sidecar--独立进程)
12. [Crate 拆分](#12-crate-拆分)
13. [技术栈](#13-技术栈)
14. [配置示例](#14-配置示例)
15. [实施路线图](#15-实施路线图)

---

## 1. 设计原则

### 核心命题：Agent 不是管道，是人

一个完整的 Agent 由五个正交维度构成。当前主流框架把它们塞进 prompt 或 scratchpad 里。
uwu_agent_engine 把它们拆成独立一等概念，各自独立 crate、独立生命周期、独立可测试性。

| 维度 | Crate | 市面做法 | uwu_agent_engine 做法 | 锋利之处 |
|---|---|---|---|---|
| **Reaction** 反应 | `agent-reaction` | 每步过 LLM | 规则触发短路，省 30-50% token | OS Agent 刚需 |
| **State** 状态 | `agent-state` | 非结构化 scratchpad | 短/中/长程 + fork() 推演沙盒 + MVCC 并发 | World Model/JEPA 有家可归 |
| **Metacognition** 元认知 | `agent-metacognition` | 无（离线学习） | 在线三信号融合自校准 + TTS | 行业未标准化 |
| **Persona** 人物角色 | `agent-persona` | 塞 persona prompt | 身份/关系/履历（可变，MVCC） | 跨组织协作的身份锚点 |
| **Character** 人格 | `agent-character` | 塞 system prompt | 核心价值观（不可变）+ 决策偏好（可调） | 安全对齐+个性化 |

- **State 唯一真相源**：所有决策基于 AgentState 短/中/长程，不是 scratchpad 文本
- **Reaction 优先**：每步决策前先过反应层，命中则短路跳过 LLM
- **Metacognition 在线三信号**：`w₁×verifier + w₂×(1-pred_error) + w₃×cost_remaining`
- **事件即契约（跨进程安全）**：`SerializedEnvelope` 替代 `Box<dyn Any>`，类型注册表确保反序列化安全
- **State MVCC**：五维并发访问通过 MVCC 版本号协调，主进程读写，Sidecar 只读快照
- **编排显式化**：`FlowGraph` 是配置，支持运行时动态扩边
- **记忆统一化**：一个向量 DB + 一个元数据 DB，四型是视图
- **能力动态加载**：`agent-core` 通过 trait object 运行时注册能力域，支持插件式扩展
- **Sidecar 独立化**：巩固（LearnNode触发+Guard博弈）和监控是独立进程
- **GuardLayer 不可绕过**：五层闸门（指令/参数/能力/预算/egress），编译期注册，不可自提升

---

## 2. 架构全景

### 2.1 决策主循环

```
每个请求步骤：

  1. Reaction.intercept(state.short_term)
     ├── Hit  → 直接执行动作（0 token）
     └── Miss → 进入 FlowGraph

  2. FlowGraph: Perception → Memory → Reasoning → Execution
     Reasoning 内：fork() State 沙盒推演候选动作 → State.evaluate() 选最优
     TTS 信号注入：根据 cost_remaining 动态调整推理策略

  3. Metacognition.evaluate(state, decision)
     meta_score = w1×verifier + w2×(1-pred_error) + w3×cost_remaining
     ├── Proceed              → 正常
     ├── RetryDecision        → 回滚 State，重新推理
     ├── RequestClarification → 暂停，向用户提问
     ├── SwitchStrategy       → 切换推理模式（含模式检测）
     └── AbortOnBudget        → 预算耗尽，终止

  4. Execution(含 GuardLayer 五层闸门) → 用结果修正 State

  5. Metacognition.calibrate_with_outcome(state, actual)
     → 更新 JEPA pred_error（EMA） → 更新校准历史 → 检测概念漂移

Persona & Character 贯穿全程：
  Persona.to_context_injection() → 注入推理上下文
  Character.preferences          → 影响决策倾向
  Character.check_core_values()  → 硬约束
```

### 2.2 抽象层级

```
Session（对话）    ← 用户会话，跨多轮，MVCC 版本化
  └── Task × N（任务） ← 持久工作单元，含 TaskManifest(AgentCard+Settlement)
        └── Subtask × M（子任务） ← 每个子任务 = 一个 FlowGraph
              └── Reaction 拦截 → FlowGraph → Metacognition → GuardLayer → 执行
```

### 2.3 系统全景

```
┌──────────────────────────────────────────────────────────────┐
│                     agent-core（主进程）                      │
│                                                              │
│  Session(MVCC)── 持有五维（Reaction/State/Metacog/Persona/   │
│     │                     Char）+ 能力注册表                   │
│     ├── Task（TaskManifest + DAG + 调度）                    │
│     │     └── FlowGraph（P→M→R→E，运行时动态扩边）           │
│     │                                                        │
│     └── 事件输出 ──► agent-mesh（SerializedEnvelope）       │
│                         │                                    │
│              ┌──────────┼──────────┐                         │
│              ▼          ▼          ▼                         │
│       Collaboration  Memory   ┌──────────────────────┐       │
│       (跨Agent委派)  (Qdrant │ NATS / JetStream     │       │
│        含 AgentCard  +PG)    │  (JetStream ack)     │       │
│        含 Settlement         │      │        │       │       │
│                              │  Consolidator  Monitor│       │
│                              │  (Sidecar)   (Sidecar)│       │
│                              │  LearnNode触发+Guard  │       │
│                              └──────────────────────┘       │
└──────────────────────────────────────────────────────────────┘
```

---

## 3. Agent 五维

### 3.1 Reaction —— 反射短路

> 每步决策前的独立拦截器。命中则短路跳过 LLM，省 30-50% token。

```rust
// agent-reaction/src/lib.rs

pub struct ReactionLayer {
    rules: Vec<Box<dyn ReactionRule + Send + Sync>>,
    stats: ReactionStats,
}

#[async_trait]
pub trait ReactionRule: Send + Sync {
    fn matches(&self, state: &AgentState) -> bool;
    async fn react(&self, state: &AgentState) -> Reaction;
}

pub enum Reaction { Hit(Action), Miss }

impl ReactionLayer {
    pub async fn intercept(&self, state: &AgentState) -> Reaction {
        for rule in &self.rules {
            if rule.matches(state) {
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Reaction::Hit(rule.react(state).await);
            }
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        Reaction::Miss
    }
}

// 内置规则
pub struct PopupCloseRule;
#[async_trait]
impl ReactionRule for PopupCloseRule {
    fn matches(&self, state: &AgentState) -> bool {
        state.short_term.current_context.ui_elements
            .iter().any(|el| el.is_popup_close_button())
    }
    async fn react(&self, state: &AgentState) -> Reaction {
        let btn = state.short_term.current_context.ui_elements
            .iter().find(|el| el.is_popup_close_button()).unwrap();
        Reaction::Hit(Action::click(btn.coordinates))
    }
}
```

---

### 3.2 State —— 结构化世界理解（MVCC 版）

> State 是 Agent 对"世界长什么样 + 任务进行到哪"的结构化理解。
> 三层 WS 独立可序列化，支持 MVCC 并发访问。
> Sidecar 读取 State 快照（只读），主进程写入时增加版本号。

#### 短/中/长程拆分（独立可序列化）

```rust
// agent-state/src/lib.rs

/// 三层 WS 独立可序列化，支持分别持久化和传输
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermWS {
    pub version: u64,  // MVCC 版本号，每步 +1
    pub current_context: ContextDescriptor,
    pub last_action: Option<Action>,
    pub last_observation: Option<String>,
    pub pending_hypotheses: Vec<Hypothesis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidTermWS {
    pub version: u64,  // MVCC 版本号，每 N 步 +1
    pub action_history: Vec<ActionRecord>,
    pub known_facts: Vec<Fact>,
    pub recent_pattern: Option<InteractionPattern>,
    pub active_constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermWS {
    pub version: u64,  // MVCC 版本号，任务级 +1
    pub task_progress: TaskProgress,
    pub accumulated_pred_error: f32,
    pub budget_consumed: BudgetConsumed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub state_id: StateId,
    pub timestamp: DateTime<Utc>,
    pub short_term: ShortTermWS,
    pub mid_term: MidTermWS,
    pub long_term: LongTermWS,
    pub confidence: ConfidenceMap,
    pub parent_state_id: Option<StateId>,
}

/// MVCC 快照：Sidecar 读取时获取此快照，不阻塞主进程写入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub snapshot_version: u64,  // 全局版本号 = max(short, mid, long).version
    pub short_term: ShortTermWS,
    pub mid_term: MidTermWS,
    pub long_term: LongTermWS,
    pub taken_at: DateTime<Utc>,
}

impl AgentState {
    /// 主进程调用：fork 时复制整个 State，不增加原 State 版本号
    pub fn fork(&self) -> Self {
        let mut s = self.clone();
        s.state_id = StateId::new();
        s.parent_state_id = Some(self.state_id);
        s
    }

    /// 主进程调用：应用动作后增加对应层的版本号
    pub fn apply_action(&mut self, action: &Action) {
        self.short_term.version += 1;
        // ... 原有 apply_hypothetical 逻辑
    }

    /// 生成 MVCC 快照供 Sidecar 读取（只读，不阻塞）
    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            snapshot_version: self.short_term.version.max(
                self.mid_term.version.max(self.long_term.version)
            ),
            short_term: self.short_term.clone(),
            mid_term: self.mid_term.clone(),
            long_term: self.long_term.clone(),
            taken_at: Utc::now(),
        }
    }
}
```

#### InteractionPattern 被 Metacognition 消费

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionPattern {
    pub recent_success_rate: f32,
    pub detected_pattern: Option<String>,
    pub pattern_since_step: u32,
}

// 消费逻辑在 Metacognition::evaluate() 中：
//   recent_success_rate < 0.3 连续 5 步 → MetaAction::SwitchStrategy
//   detected_pattern == "loop_detected"  → MetaAction::SwitchStrategy
```

#### 推演沙盒 + JEPA 预测误差

```rust
impl AgentState {
    pub fn apply_hypothetical(&mut self, action: &Action) {
        self.mid_term.action_history.push(ActionRecord {
            action: action.clone(),
            status: ActionStatus::Hypothetical,
            timestamp: Utc::now(),
        });
        // ... 原有逻辑
    }

    pub fn compute_pred_error(&self, actual: &Self) -> f32 {
        let total = self.mid_term.known_facts.len().max(1) as f32;
        let diff = self.diff(actual);
        ((diff.facts_added.len() + diff.facts_modified.len()) as f32 / total).clamp(0.0, 1.0)
    }

    pub fn update_pred_error(&mut self, actual: &Self) {
        let err = self.compute_pred_error(actual);
        self.long_term.accumulated_pred_error =
            0.3 * err + 0.7 * self.long_term.accumulated_pred_error;
        self.long_term.version += 1;  // MVCC：长程版本号增加
    }

    pub fn evaluate(&self) -> StateScore { /* 原有逻辑 */ }
    pub fn diff(&self, other: &Self) -> StateDiff { /* ... */ }
    pub fn checkpoint(&self) -> StateCheckpoint { /* ... */ }
    pub fn rollback(checkpoint: &StateCheckpoint) -> Self { /* ... */ }
}
```

---

### 3.3 Metacognition —— 三信号在线自校准

> 单 Verifier 有自我闭环风险。融合三路独立信号：

```
meta_score = w1 × verifier + w2 × (1 - pred_error) + w3 × cost_remaining
w1=0.5, w2=0.3, w3=0.2（可配置）
```

- **Verifier**：独立校准模型，评估决策质量
- **pred_error**：JEPA 预测误差，来自 State.long_term，是环境真实反馈
- **cost_remaining**：剩余预算比例，防止长任务无节制消耗

```rust
// agent-metacognition/src/lib.rs

pub struct Metacognition {
    calibration_model: Box<dyn CalibrationModel>,
    calibration_history: Vec<CalibrationRecord>,
    anomaly_detector: AnomalyDetector,
    weights: MetaScoreWeights,
    token_budget: u64, time_budget: f64, retry_budget: u32,
}

pub struct MetaScoreWeights { pub verifier: f32, pub pred_error: f32, pub cost_remaining: f32 }

#[async_trait]
pub trait CalibrationModel: Send + Sync {
    async fn calibrate(&self, state: &AgentState, decision: &Decision) -> CalibrationResult;
}

pub struct CalibrationResult {
    pub raw_confidence: f32, pub calibrated_confidence: f32,
    pub should_retry: bool, pub reasoning: String,
}

pub enum MetaAction { Proceed, RetryDecision, RequestClarification, SwitchStrategy, DelegateToHuman, AbortOnBudget }

impl Metacognition {
    /// 三信号融合评估。pred_error 来自 State 推演沙盒，不消耗 LLM call。
    /// verifier 可用本地小模型（Qwen2.5-0.5B），单次推理 < 50ms，成本约为 LLM call 的 1/100。
    pub async fn evaluate(&self, state: &AgentState, decision: &Decision) -> MetacognitiveAssessment {
        let cal = self.calibration_model.calibrate(state, decision).await;
        let verifier = cal.calibrated_confidence;
        let pred = 1.0 - state.long_term.accumulated_pred_error;
        let cost = self.compute_cost_remaining(&state.long_term.budget_consumed);
        let meta = self.weights.verifier * verifier
            + self.weights.pred_error * pred
            + self.weights.cost_remaining * cost;

        // 消费 MidTermWS.recent_pattern：元认知"看见模式"
        let pattern_loop = state.mid_term.recent_pattern.as_ref()
            .map(|p| p.detected_pattern.as_deref() == Some("loop_detected"))
            .unwrap_or(false);
        let low_success = state.mid_term.recent_pattern.as_ref()
            .map(|p| p.recent_success_rate < 0.3 && p.pattern_since_step >= 5)
            .unwrap_or(false);

        let knows_unknown = meta < 0.4;
        let drifting = self.anomaly_detector.detect_drift(&self.calibration_history);
        let budget_exhausted = cost < 0.05;

        MetacognitiveAssessment {
            calibration: cal, meta_score: meta,
            knows_unknown, concept_drifting: drifting, budget_exhausted,
            suggested_action: if budget_exhausted { MetaAction::AbortOnBudget }
            else if pattern_loop || low_success { MetaAction::SwitchStrategy }
            else if knows_unknown { MetaAction::RequestClarification }
            else if drifting { MetaAction::SwitchStrategy }
            else if cal.should_retry { MetaAction::RetryDecision }
            else { MetaAction::Proceed },
        }
    }

    pub fn calibrate_with_outcome(&mut self, state: &mut AgentState, actual: &AgentState) {
        state.update_pred_error(actual);
        self.calibration_history.push(CalibrationRecord {
            predicted_state_id: state.state_id, actual_state_id: actual.state_id,
            diff: state.diff(actual), timestamp: Utc::now(),
        });
        self.anomaly_detector.update(&self.calibration_history);
    }

    fn compute_cost_remaining(&self, b: &BudgetConsumed) -> f32 {
        let t = 1.0 - (b.tokens_used as f32 / self.token_budget.max(1) as f32);
        let tm = 1.0 - (b.elapsed.as_secs_f32() / self.time_budget.max(1.0));
        let r = 1.0 - (b.retries as f32 / self.retry_budget.max(1) as f32);
        t.min(tm).min(r).clamp(0.0, 1.0)
    }

    /// TTS（Time To Stop）信号：渐进式预算压力注入决策
    /// 不是只有耗尽才停，而是在预算消耗到阈值时主动调整推理策略：
    ///   cost_remaining > 0.5  → 正常推理（ToT beam search 允许）
    ///   0.2 ≤ cost_remaining ≤ 0.5 → 降级：禁用 ToT，切换为单步推理
    ///   0.05 ≤ cost_remaining < 0.2 → 紧急：只走 Reaction 短路 + 直接回答，禁止新工具调用
    ///   cost_remaining < 0.05 → AbortOnBudget
    pub fn tts_signal(&self, b: &BudgetConsumed) -> TTSSignal {
        let c = self.compute_cost_remaining(b);
        match c {
            c if c < 0.05 => TTSSignal::Abort,
            c if c < 0.2  => TTSSignal::Urgent { allow_reaction: true, allow_new_tool: false },
            c if c < 0.5  => TTSSignal::Degraded { disable_tot: true },
            _             => TTSSignal::Normal,
        }
    }
}

pub enum TTSSignal {
    Normal,
    Degraded { disable_tot: bool },
    Urgent { allow_reaction: bool, allow_new_tool: bool },
    Abort,
}

// 在线 Metacognition vs Sidecar Monitor
// ┌─────────────────┬────────────────────────┬──────────────────────────┐
// │                 │ Metacognition（在线）   │ Monitor（Sidecar）       │
// ├─────────────────┼────────────────────────┼──────────────────────────┤
// │ 触发            │ 每步决策后              │ 异步、节流（60s+异常）   │
// │ 延迟要求         │ < 200ms               │ 无要求                   │
// │ 职责            │ 单步校准："这一步对吗？" │ 全局检测："最近在退化吗？" │
// │ 输出            │ MetaAction             │ MetacognitiveReport      │
// └─────────────────┴────────────────────────┴──────────────────────────┘

// 三信号成本分析：
// ┌─────────────┬──────────────┬─────────────────────────────────┐
// │ 信号         │ 来源          │ 成本                             │
// ├─────────────┼──────────────┼─────────────────────────────────┤
// │ verifier     │ 本地校准模型  │ ~50ms，无 LLM call（可用Qwen2.5-0.5B）│
// │ pred_error   │ State fork() │ 零 LLM call，纯本地计算           │
// │ cost_remaining│ 计数器       │ 零 LLM call，纯本地计算           │
// └─────────────┴──────────────┴─────────────────────────────────┘
// 结论：三信号融合每步额外成本 < 100ms，不改变 LLM 调用次数。
//       省 LLM 的核心来自 Reaction 层短路（命中则 0 token）。
```

---

### 3.4 Persona —— 人物角色（可变，MVCC）

> Agent 的"我是谁"——身份、关系网络、履历。随经历增长而变化。
> Persona 更新通过 MVCC 快照，Sidecar 只读，主进程写入。

```rust
// agent-persona/src/lib.rs

pub struct Persona {
    pub version: u64,  // MVCC 版本号
    pub identity: Identity,
    pub relationships: RelationshipGraph,
    pub history: PersonaHistory,
}

impl Persona {
    pub fn to_context_injection(&self) -> PersonaContext { /* ... */ }
    pub fn update_relationship(&mut self, peer: AgentId, outcome: &CollaborationOutcome) {
        self.version += 1;
        /* 原有逻辑 */
    }
    /// 生成快照供 Sidecar 读取
    pub fn snapshot(&self) -> PersonaSnapshot { /* ... */ }
}
```

---

### 3.5 Character —— 人格（核心价值观不可变 + 偏好可调）

> Character 是 Agent 的"性格"——底层不可变的核心价值观（安全锚点），上层可调整的决策偏好。

| 层 | 可变性 | 内容 |
|---|---|---|
| 核心价值观 | **不可变** | "不泄露隐私"、"不执行破坏性命令"、"诚实优先于讨好" |
| 决策偏好 | 可调整 | 工具偏好、风险容忍度、不确定策略、输出风格 |

```rust
// agent-character/src/lib.rs

pub struct Character {
    pub core_values: Vec<CoreValue>,
    pub preferences: Preferences,
}

pub struct CoreValue { pub name: String, pub description: String, pub enforcement: ValueEnforcement }
pub enum ValueEnforcement { HardConstraint, SoftGuideline }
pub struct Preferences { pub tool_preference: Vec<String>, pub risk_tolerance: f32, pub uncertainty_strategy: UncertaintyStrategy, pub output_style: OutputStyle }
pub enum UncertaintyStrategy { SearchFirst, AskUserFirst, BestGuessAndConfirm }
pub enum OutputStyle { Concise, Detailed, StepByStep }

impl Character {
    pub fn to_context_injection(&self) -> CharacterContext { /* ... */ }
    pub fn check_core_values(&self, action: &Action) -> Result<(), ValueViolation> {
        for v in &self.core_values {
            if v.enforcement == ValueEnforcement::HardConstraint && v.violates(action) {
                return Err(ValueViolation { value: v.name.clone(), action: action.clone(), reason: v.description.clone() });
            }
        }
        Ok(())
    }
}
```

### 三层约束体系

```
Character.core_values（HardConstraint） → 决策层约束
Persona.relationships                   → 社交层约束
GuardLayer（硬闸门）                    → 执行层约束
```

---

## 4. Session + MVCC 并发模型

> Session 持有五维，通过 MVCC 版本号协调主进程和 Sidecar 的并发访问。

```
并发模型：
  主进程：读写五维，每次写入增加对应层的 version
  Sidecar：读取快照（StateSnapshot + PersonaSnapshot），不阻塞主进程
  事件网格：通过 NATS JetStream 传输序列化快照，Sidecar 消费后本地恢复
```

```rust
// agent-session/src/lib.rs

pub struct Session {
    pub session_id: SessionId, pub user_id: UserId,
    pub reaction: Arc<ReactionLayer>,
    pub state: AgentState,          // 主进程拥有，MVCC 写入者
    pub metacognition: Arc<Metacognition>,
    pub persona: Persona,           // MVCC 写入者
    pub character: Arc<Character>,   // 只读（Character 不可变）
    pub history: Vec<ConversationTurn>,
    pub output_stream: Option<Box<dyn OutputStream>>,
    pub intent_tracker: IntentTracker,
    pub checkpoints: Vec<StateCheckpoint>,
    pub active_tasks: Vec<TaskHandle>, pub completed_tasks: Vec<TaskSummary>,
    pub created_at: DateTime<Utc>, pub last_active_at: DateTime<Utc>,
    // 能力注册表：运行时动态注册能力域
    pub capability_registry: CapabilityRegistry,
}

/// 能力动态注册：agent-core 不依赖具体能力域 crate，通过 trait object 注册
pub struct CapabilityRegistry {
    perceivers:  Vec<Box<dyn Perceiver>>,
    reasoners:   Vec<Box<dyn Reasoner>>,
    executors:   Vec<Box<dyn Executor>>,
}

impl CapabilityRegistry {
    pub fn register_perceiver(&mut self, p: Box<dyn Perceiver>) { self.perceivers.push(p); }
    pub fn register_reasoner(&mut self, r: Box<dyn Reasoner>) { self.reasoners.push(r); }
    pub fn register_executor(&mut self, e: Box<dyn Executor>) { self.executors.push(e); }
}

impl Session {
    pub async fn process_turn(&mut self, raw: RawInput) -> FinalOutput {
        let input = self.enrich_input(raw);
        if let Reaction::Hit(action) = self.reaction.intercept(&self.state).await {
            return self.execute_reaction(action).await;
        }
        let decision = self.run_flowgraph(input).await;
        let assessment = self.metacognition.evaluate(&self.state, &decision).await;
        match assessment.suggested_action {
            MetaAction::Proceed => {},
            MetaAction::RetryDecision => { /* 回滚 State，重新推理 */ },
            MetaAction::RequestClarification => { /* 暂停，向用户提问 */ },
            MetaAction::SwitchStrategy => { /* 切换推理模式 */ },
            MetaAction::DelegateToHuman => { /* 升级 */ },
            MetaAction::AbortOnBudget => { /* 预算耗尽，终止 */ },
        }
        let result = self.execute_and_update(decision).await;
        self.metacognition.calibrate_with_outcome(&mut self.state, &result.actual_state);
        result.output
    }

    /// 生成快照供 Sidecar 消费（通过事件网格发送）
    pub fn emit_snapshot(&self) -> SerializedEnvelope<StateSnapshot> {
        let snapshot = self.state.snapshot();
        SerializedEnvelope::new("state.snapshot", snapshot)
    }
}
```

---

## 5. Task —— 任务域

Task 是跨多轮、可能跨多 Agent 的持久工作单元。

```rust
// agent-task/src/lib.rs

pub struct Task {
    pub task_id: TaskId, pub goal: Goal, pub status: TaskStatus,
    pub subtask_dag: SubtaskDag, pub max_retries_per_subtask: u32,
    pub manifest: TaskManifest,
}

pub struct TaskManifest {
    pub participants: Vec<AgentCard>,
    pub delegation_policy: DelegationPolicy,
    pub settlement: Option<SettlementPolicy>,
}

pub struct AgentCard { pub agent_id: AgentId, pub name: String, pub capabilities: Vec<String>, pub role: TaskRole, pub priority: u8, pub endpoint: AgentEndpoint }
pub enum TaskRole { Orchestrator, Executor, Observer }
pub struct DelegationPolicy { pub discovery: DiscoveryStrategy, pub delegation_timeout: Duration, pub fallback: FallbackStrategy }
pub enum DiscoveryStrategy { ExactCapability, LoadBalanced, TrustRanked, Auction }
pub enum FallbackStrategy { ExecuteLocally, SkipSubtask, AskUser }

pub struct SettlementPolicy { pub mode: SettlementMode, pub budget_cap: Option<Budget>, pub unit: String }
pub enum SettlementMode { FreeCollaboration, FixedPrice { price_per_subtask: f64 }, Metered { rate_per_token: f64, rate_per_second: f64 }, Auction { max_bid: f64 } }

pub struct SubtaskDag { pub nodes: Vec<Subtask>, pub edges: Vec<SubtaskEdge> }
pub struct Subtask { pub id: SubtaskId, pub description: String, pub status: SubtaskStatus, pub assigned_agent: Option<AgentId>, pub flow_graph: FlowGraph, pub max_retries: u32, pub timeout: Duration }
pub enum SubtaskStatus { Pending, Ready, Running, Delegated(AgentId), Completed { result: SubtaskResult }, Failed { error: String, retries: u32 }, Skipped }

impl Task {
    pub fn check_ready(&self, state: &AgentState) -> Vec<SubtaskId> { /* ... */ }
    pub fn update_progress(&self, state: &mut AgentState) { /* ... */ }
}
```

---

## 6. Collaboration —— 多 Agent 协作域

```rust
// agent-collaboration/src/lib.rs

pub struct Collaboration {
    registry: Arc<AgentRegistry>,
    mesh: Arc<dyn EventMesh>,
    pending: Arc<DashMap<DelegationId, DelegationState>>,
}

pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<AgentId, AgentDescriptor>>>,
    capability_index: Arc<RwLock<HashMap<String, Vec<AgentId>>>>,
}

pub struct AgentDescriptor { pub agent_id: AgentId, pub name: String, pub capabilities: Vec<String>, pub load: f32, pub health: HealthStatus, pub endpoint: AgentEndpoint }

impl Collaboration {
    pub async fn delegate(&self, subtask: &Subtask, to: &AgentId, state_snapshot: &AgentState, persona_context: &PersonaContext) -> DelegationResult { /* ... */ }
    pub async fn on_delegation_complete(&self, delegation_id: DelegationId, result: SubtaskResult, state_delta: StateDiff) { /* ... */ }
    pub async fn negotiate(&self, key: &str, a: Value, agent_a: AgentId, b: Value, agent_b: AgentId) -> Value { /* ... */ }
}
```

---

## 7. FlowGraph —— 动态数据流

> FlowGraph 是 Agent 决策管道的领域抽象。底层执行引擎使用 **uwu_visual_script**（Graph → ExecutionPlan → SlotProgram → VM），
> 无需重复造轮子。FlowGraph 专注于 Agent 语义（Perception → Memory → Reasoning → Execution），
> visual_script 负责编译、类型校验、step budget、取消、序列化等通用图执行基础设施。

### 7.1 两层架构

```
┌──────────────────────────────────────────────┐
│  FlowGraph（领域层 / agent-core）              │
│  - Perception / Memory / Reasoning / Execution │
│  - 动态扩边（验证回边）                         │
│  - 高安全模式                                  │
│  - FlowConfig → Graph 工厂方法                 │
└──────────────────┬───────────────────────────┘
                   │ 编译为
                   ▼
┌──────────────────────────────────────────────┐
│  uwu_visual_script（执行层）                   │
│  - Graph → ExecutionPlan → SlotProgram        │
│  - VM 解释执行（同步 / 异步）                   │
│  - Step budget + Cancel + Middleware          │
│  - ExecutionPlan 跨进程 serde                  │
└──────────────────────────────────────────────┘
```

### 7.2 领域包装设计

```rust
// agent-core/src/flow.rs

use uwu_visual_script::{Graph, NodeDefRef, NodeId, compile_to_plan, instantiate};
use std::sync::Arc;

/// FlowGraph 是 Agent 决策管道的领域抽象。
/// 内部使用 uwu_visual_script 的 Graph 作为数据结构，
/// 通过 NodeRegistry 注册 Agent 能力节点（Perception / Memory / Reasoning / Execution）。
pub struct FlowGraph {
    /// 底层 visual_script 图（可序列化、可跨进程传输）
    inner: Graph,
    /// 已实例化的可执行程序（惰性实例化，重新编译时替换）
    program: Option<Arc<SlotProgram>>,
}

/// Agent 能力节点类型（注册到 visual_script NodeLibrary）
pub enum AgentCapability {
    Perception,
    Memory,
    Reasoning,
    Execution,
}

impl FlowGraph {
    /// 标准 P→M→R→E 管道
    pub fn standard(lib: &NodeLibrary) -> Self {
        let mut g = Graph::new("agent-decision-loop");
        let p = g.add_node(NodeDefRef::new("perception.observe"));
        let m = g.add_node(NodeDefRef::new("memory.retrieve"));
        let r = g.add_node(NodeDefRef::new("reasoning.decide"));
        let e = g.add_node(NodeDefRef::new("execution.act"));
        // exec 流: P → M → R → E
        g.chain_exec(&[p, m, r, e]);
        let plan = compile_to_plan(&g, lib).expect("standard pipeline must compile");
        let program = instantiate(&plan, lib).expect("standard pipeline must instantiate");
        Self { inner: g, program: Some(Arc::new(program)) }
    }

    /// 运行时动态扩边：克隆图 → 添加新边 → 重新编译 → 替换 program
    /// 典型场景：高安全模式下推理后自动添加验证回边
    pub fn add_edge_dynamic(&mut self, edge: FlowEdge, lib: &NodeLibrary) {
        self.inner.edges.push(edge.into());
        let plan = compile_to_plan(&self.inner, lib)
            .expect("dynamic edge must produce valid graph");
        self.program = Some(Arc::new(instantiate(&plan, lib)
            .expect("dynamic edge must instantiate")));
    }

    /// 高安全模式：推理后自动添加验证回边
    /// Reasoning.decision → Reasoning.validate（自检循环）
    pub fn high_security(lib: &NodeLibrary) -> Self {
        let mut g = Self::standard(lib);
        // reasoner 输出 raw_decision 后立即触发自身的 validate 入口
        g.add_edge_dynamic(
            FlowEdge::new("n3", "reasoning.decision.raw", "n3", "reasoning.validate"),
            lib,
        );
        g
    }

    /// 从配置文件构建 FlowGraph
    pub fn from_config(config: &FlowConfig, lib: &NodeLibrary) -> Self { /* ... */ }

    /// 获取当前可执行程序
    pub fn program(&self) -> &Arc<SlotProgram> {
        self.program.as_ref().expect("FlowGraph not instantiated")
    }

    /// 导出为可序列化的 ExecutionPlan（跨进程传输给 Sidecar）
    pub fn to_plan(&self) -> ExecutionPlan {
        self.program().to_plan()
    }
}

/// Agent 能力节点通过 visual_script 的 NodeDefinition 注册。
/// 每个能力封装为 Impure + Async runner，由 Session 的 CapabilityRegistry 注入。
///
/// 示例：Perception 节点定义
/// ```rust
/// NodeDefinition {
///     id: "perception.observe".into(),
///     purity: Purity::Impure,
///     inputs: vec![exec_in(), data_in("raw_input", ValueType::String, None)],
///     outputs: vec![exec_out("done"), data_out("context", ValueType::Json)],
///     runner: RunnerKind::async(PerceptionRunner),
/// }
/// ```
```

### 7.3 动态扩边的两种路径

| 路径 | 场景 | 方法 |
|---|---|---|
| **闭包验证** | 高安全模式，推理后自检 | `FlowGraph::high_security()` — 编译期预设 |
| **运行时学习** | LearnNode 发现新模式，插入新能力节点 | `add_edge_dynamic()` → 重新编译 → 原子替换 `program` |

与原始设计的关键变化：动态边不再存储在 `RwLock<Vec<FlowEdge>>` 中，而是直接写入 `Graph.edges`，
然后重新编译生成新的 `SlotProgram`。visual_script 的 `ExecutionPlan` 已支持 serde，
重新编译后可跨进程传输给 Sidecar 执行。

---

## 8. agent-mesh —— 事件网格（跨进程安全）

> `Box<dyn Any>` 改为 `SerializedEnvelope<T>` + 类型注册表。
> 解决 Sidecar 独立进程无法反序列化 `dyn Any` 的问题。

```rust
// agent-mesh/src/envelope.rs

/// 类型注册表：确保反序列化安全，防止未知类型注入
pub struct TypeRegistry {
    entries: RwLock<HashMap<TypeId, Box<dyn Fn(Vec<u8>) -> Result<Box<dyn Any + Send + Sync>, Error> + Send + Sync>>>,
}

impl TypeRegistry {
    pub fn register<T: Serialize + DeserializeOwned + 'static>(&self) {
        let mut map = self.entries.write().unwrap();
        map.insert(TypeId::of::<T>(), Box::new(|bytes| {
            Ok(Box::new(serde_json::from_slice::<T>(&bytes)?) as Box<dyn Any + Send + Sync>)
        }));
    }

    pub fn deserialize(&self, type_id: &TypeId, bytes: &[u8]) -> Result<Box<dyn Any + Send + Sync>, Error> {
        let map = self.entries.read().unwrap();
        let factory = map.get(type_id).ok_or_else(|| Error::UnknownType(type_id.clone()))?;
        factory(bytes)
    }
}

/// 跨进程安全的序列化信封（替代 Box<dyn Any>）
#[derive(Serialize, Deserialize)]
pub struct SerializedEnvelope {
    pub type_id: TypeId,
    pub correlation_id: CorrelationId,
    pub sequence_number: u64,
    pub replay_id: Option<ReplayId>,
    pub payload_bytes: Vec<u8>,  // 序列化的 payload，跨进程可传输
    pub metadata: EventMetadata,
}

impl SerializedEnvelope {
    pub fn new<T: Serialize>(type_name: &str, payload: T) -> Self {
        SerializedEnvelope {
            type_id: TypeId::new(type_name),
            correlation_id: CorrelationId::new(),
            sequence_number: 0,
            replay_id: None,
            payload_bytes: serde_json::to_vec(&payload).unwrap(),
            metadata: EventMetadata::new(),
        }
    }

    /// 反序列化 payload（需要类型注册表）
    pub fn deserialize_payload<T: DeserializeOwned>(&self) -> Result<T, Error> {
        Ok(serde_json::from_slice(&self.payload_bytes)?)
    }
}

pub struct TypeId { pub domain: String, pub event: String }
pub struct EventMetadata { pub produced_at: DateTime<Utc>, pub producer_id: AgentId, pub ttl: Option<Duration> }

/// Crash Recovery 策略：
/// 1. NATS JetStream 持久化：所有事件写入 JetStream，consumer 有 ack 机制
/// 2. SequenceNumber 连续性检查：consumer 发现 gap → 请求重传（不自行补全）
/// 3. Replay：agent 重启后，从最后一个 ack 的 sequence_number 开始重放
///    replay_id 标记重放事件 → 消费者见到 replay_id.is_some() 则跳过副作用操作
/// 4. 分叉防护：State.checkpoint() 在每次外部副作用前持久化 →
///    crash 后从 checkpoint 恢复，不重复执行副作用
pub type ReplayId = String;
pub type CorrelationId = String;
```

```rust
// agent-mesh/src/flow.rs

pub struct FlowHandle {
    pub correlation_id: CorrelationId,
    seq: AtomicU64,
    type_registry: Arc<TypeRegistry>,  // 类型注册表，确保跨进程反序列化安全
    main_tx: mpsc::Sender<SerializedEnvelope>,
    consolidation_tx: mpsc::Sender<SerializedEnvelope>,
    monitoring_tx: mpsc::Sender<SerializedEnvelope>,
    system_tx: mpsc::Sender<SerializedEnvelope>,
}

impl FlowHandle {
    pub async fn publish<T: Serialize>(&self, event_type: &str, payload: T) -> Result<(), Error> {
        let mut envelope = SerializedEnvelope::new(event_type, payload);
        envelope.sequence_number = self.seq.fetch_add(1, Ordering::SeqCst);
        // 自动路由到 Main/Consolidation/Monitoring/System
        Ok(())
    }
    pub fn cancel(self) { drop(self); }
}

// 四路通道：Main(64) / Consolidation(256) / Monitoring(64) / System(128)
```

---

## 9. UnifiedMemory —— 统一记忆

一个向量 DB（Qdrant）+ 一个元数据 DB（PostgreSQL）。四型是查询视图。LearnNode 触发学习时 GuardLayer 博弈检查 SkillTarget。

```rust
// agent-memory/src/unified.rs

pub struct UnifiedMemory {
    vector_db: Arc<dyn VectorStore>,
    metadata_db: PgPool,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn search(&self, query: &Embedding, opts: &SearchOpts) -> Vec<Memory>;
    async fn upsert(&self, id: MemoryId, embedding: Embedding, metadata: &Value);
}

impl UnifiedMemory {
    pub async fn retrieve(&self, intent: &RetrievalIntent) -> RetrievedMemories { /* 80% 场景 */ }
    pub async fn retrieve_typed(&self, intent: &RetrievalIntent, types: &[MemoryType]) -> HashMap<MemoryType, Vec<Memory>> { /* 按需降级 */ }
    pub async fn persist_state(&self, snapshot: &StateSnapshot) { /* ... */ }
    pub async fn persist_persona(&self, snapshot: &PersonaSnapshot) { /* ... */ }
    pub async fn consolidate(&self, episode: Episode) { /* ... */ }
}

pub struct Memory { pub id: MemoryId, pub memory_type: MemoryType, pub content: String, pub embedding: Vec<f32>, pub score: MemoryScore, pub state_snapshot: Option<StateSnapshot> }
```

### LearnNode 触发条件与 Guard 博弈

```rust
// agent-learning/src/trigger.rs

pub struct LearnTrigger { conditions: Vec<Box<dyn LearnCondition + Send + Sync>> }

#[async_trait]
pub trait LearnCondition: Send + Sync {
    async fn should_learn(&self, episode: &Episode, state: &AgentState) -> LearnDecision;
}

pub enum LearnDecision {
    Skip,
    ConsolidateEpisode,
    ExtractSkill { skill_name: String, skill_target: SkillTarget, confidence: f32 },
    UpdatePreference { field: String, new_value: Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillTarget {
    LocalCode { crate_name: String },
    McpRemote { server_id: String, tool_name: String, endpoint: String },
}

/// 学习成果版本化：每次 ExtractSkill 生成新版本，支持回滚
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub version_id: String,
    pub skill_name: String,
    pub target: SkillTarget,
    pub created_at: DateTime<Utc>,
    pub episode_id: EpisodeId,
    pub hash: String,  // 代码/配置的 hash，用于完整性校验
    pub verified: bool,  // 沙箱验证是否通过
    pub active: bool,
}

impl SkillVersion {
    pub fn new(skill_name: String, target: SkillTarget, episode_id: EpisodeId) -> Self {
        let now = Utc::now();
        SkillVersion {
            version_id: format!("{}-{}", skill_name, now.timestamp()),
            skill_name, target, created_at: now, episode_id,
            hash: "".into(), verified: false, active: false,
        }
    }
}
```

```
LearnNode 流程：
  Episode 完成 → LearnTrigger.should_learn()
    ├── ExtractSkill { target: LocalCode } → 直接写入本地
    └── ExtractSkill { target: McpRemote } →
          GuardLayer.check_egress(target)
            ├── 通过 → 写入 MCP Server
            └── 拒绝 → 降级 LocalCode 或放弃

自进化失控防护：
  1. 版本化：每次 ExtractSkill 写入时生成 SkillVersion（hash + timestamp + episode_id）
  2. 沙箱验证：新 Skill 先在 fork() 的 State 沙盒中运行，通过后才会注册
  3. 回滚机制：GuardLayer 检测到异常 → 自动回滚至上一 SkillVersion
  4. 人工审批门槛：McpRemote 写入必须经过 explicit_user_approval = true
  5. 配置开关：mcp_skill_write_enabled = false（默认关闭）
```

---

## 10. GuardLayer —— 安全守卫

五层硬闸门：指令/参数/能力/预算/egress。编译期注册，不可自提升。

```rust
// agent-guard/src/lib.rs

pub struct GuardLayer {
    instruction_rules: Vec<Box<dyn InstructionRule + Send + Sync>>,
    parameter_rules:   Vec<Box<dyn ParameterRule + Send + Sync>>,
    capability_rules:  Vec<Box<dyn CapabilityRule + Send + Sync>>,
    budget_rules:      Vec<Box<dyn BudgetRule + Send + Sync>>,
    egress_rules:      Vec<Box<dyn EgressRule + Send + Sync>>,
    audit_log: Arc<AuditLog>,
}

#[async_trait] pub trait InstructionRule: Send + Sync { async fn check(&self, action: &Action) -> Option<GuardViolation>; }
#[async_trait] pub trait ParameterRule: Send + Sync { async fn check(&self, action: &Action, params: &ActionParams) -> Option<GuardViolation>; }
#[async_trait] pub trait CapabilityRule: Send + Sync { async fn check(&self, action: &Action, context: &AgentContext) -> Option<GuardViolation>; }
#[async_trait] pub trait BudgetRule: Send + Sync { async fn check(&self, budget: &BudgetConsumed, limits: &BudgetLimits) -> Option<GuardViolation>; }
#[async_trait] pub trait EgressRule: Send + Sync { async fn check_egress(&self, target: &SkillTarget, context: &AgentContext) -> Option<GuardViolation>; }

impl GuardLayer {
    pub fn builder() -> GuardBuilder { GuardBuilder::new() }

    pub async fn enforce(&self, actions: &[Action], context: &AgentContext) -> Result<Vec<Action>, Vec<GuardViolation>> {
        let mut allowed = Vec::new(); let mut blocked = Vec::new();
        for action in actions {
            let violations: Vec<_> = [
                self.check_instruction(action).await, self.check_parameters(action).await,
                self.check_capability(action, context).await,
                self.check_budget(&context.budget_consumed, &context.budget_limits).await,
            ].into_iter().flatten().collect();
            if violations.is_empty() { allowed.push(action.clone()); }
            else { self.audit_log.log_guard_hit(action, &violations).await; blocked.extend(violations); }
        }
        if blocked.is_empty() { Ok(allowed) } else { Err(blocked) }
    }

    pub async fn check_egress(&self, target: &SkillTarget, context: &AgentContext) -> Result<(), GuardViolation> { /* ... */ }
}

// 内置规则
pub struct NoRmRfRule;
#[async_trait] impl InstructionRule for NoRmRfRule { async fn check(&self, a: &Action) -> Option<GuardViolation> { if a.command.contains("rm -rf") { Some(GuardViolation { rule: "no-rm-rf", level: ViolationLevel::Critical, message: "禁止递归删除".into() }) } else { None } } }

pub struct TokenBudgetRule;
#[async_trait] impl BudgetRule for TokenBudgetRule { async fn check(&self, b: &BudgetConsumed, l: &BudgetLimits) -> Option<GuardViolation> { if b.tokens_used >= l.max_tokens { Some(GuardViolation { rule: "token-budget", level: ViolationLevel::Warning, message: format!("Token耗尽: {}/{}", b.tokens_used, l.max_tokens) }) } else { None } } }

pub struct McpWriteAllowlistRule { allowed: HashSet<String> }
#[async_trait] impl EgressRule for McpWriteAllowlistRule { async fn check_egress(&self, t: &SkillTarget, _: &AgentContext) -> Option<GuardViolation> { match t { SkillTarget::McpRemote { server_id, .. } if !self.allowed.contains(server_id) => Some(GuardViolation { rule: "mcp-write-allowlist", level: ViolationLevel::Critical, message: format!("禁止写入未授权 MCP Server: {}", server_id) }), _ => None } } }
```

---

## 11. Sidecar —— 独立进程

```
主进程 → NATS/JetStream → consolidator（LearnNode触发→Guard博弈→State/Persona持久化）
                         → monitor（Metacognition 异常检测 + State 漂移检测）
```

```rust
// agent-sidecar-consolidator/src/main.rs
#[tokio::main] async fn main() {
    let cfg = ConsolidatorConfig::from_env();
    let client = async_nats::connect(&cfg.nats_url).await.unwrap();
    let mut sub = client.subscribe("uwu_agent_engine.events.completions".to_string()).await.unwrap();
    let memory = UnifiedMemory::connect(&cfg.memory).await;
    let guard = GuardLayer::from_config(&cfg.guard);
    let learn_trigger = LearnTrigger::from_config(&cfg.learning);
    let type_registry = TypeRegistry::new();  // 初始化类型注册表

    while let Some(msg) = sub.next().await {
        let envelope: SerializedEnvelope = serde_json::from_slice(&msg.payload).unwrap();
        let episode: Episode = envelope.deserialize_payload().unwrap();
        let decision = learn_trigger.evaluate(&episode);
        match decision {
            LearnDecision::ExtractSkill { target: SkillTarget::McpRemote { .. }, .. } => {
                if guard.check_egress(&target, &episode.context).await.is_ok() {
                    memory.consolidate(episode).await;
                }
            }
            _ => { memory.consolidate(episode).await; }
        }
    }
}

// agent-sidecar-monitor/src/main.rs
#[tokio::main] async fn main() { /* 消费 uwu_agent_engine.metrics.> → 异常检测 → MetacognitiveReport */ }
```

---

## 12. Crate 拆分

### 12.1 依赖关系图

```
                            ┌─ uwu_visual_script ─┐  ← 图执行引擎（Graph → IR → VM）
                            │                     │
agent-mesh ─────────────────┼─────────────────────┤
    ↑                       │                     │
    ├── agent-reaction      ★ 反射短路
    ├── agent-state         ★ 短/中/长程 + fork/diff/rollback + MVCC
    ├── agent-metacognition ★ 三信号在线自校准 + TTS
    ├── agent-persona       ★ 身份/关系/履历（MVCC）
    ├── agent-character     ★ 核心价值观不可变 + 偏好可调
    ├── agent-session        对话域（持有五维 + MVCC + 能力注册表）
    ├── agent-task           任务域（TaskManifest + DAG + 调度）
    ├── agent-collaboration  多 Agent 协作（委派 + 协商 + CRDT）
    ├── agent-perception     感知域（输入解析 + PII）
    ├── agent-memory         统一记忆（向量 + 元数据 + LearnNode触发）
    ├── agent-reasoning      推理域（消费 State + fork()推演 + ToT）
    ├── agent-execution      执行域（MCP + Guard + 输出，可用 uwu_wasm 沙箱）
    ├── agent-guard          五层闸门（指令/参数/能力/预算/egress）
    ├── agent-learning       学习触发（LearnCondition + SkillTarget + Guard博弈）
    ├── agent-uncertainty    贝叶斯不确定性（集成到主循环）
    ├── agent-crdt           CRDT 状态
    ├── agent-tools          MCP 工具协议
    └── agent-core           会话管理 + FlowGraph(基于 uwu_visual_script) + 能力注册表

agent-sidecar-consolidator   独立巩固（LearnNode+Guard）
agent-sidecar-monitor        独立监控
agent-types-core             基础类型（冻结）
agent-types-ext              业务类型（可迭代）
agent-state                  短/中/长程 WS（合并为单一 crate）

uwu_visual_script            可视化脚本引擎（FlowGraph 执行基础设施）
uwu_wasm                     WASM 沙箱引擎（Component Model + WASI p2 + 安全策略）
uwu_database                 统一数据访问层（SQL + Cache + VectorStore）
uwu_event_mesh               事件网格（agent-mesh 的底层实现）
uwu_logger                   日志系统
```

### 12.2 详细 Crate 表

| Crate | 职责 | 关键 Trait | 依赖 |
|---|---|---|---|
| **Agent 核心** | | | |
| `agent-mesh` | 事件网格（SerializedEnvelope + 类型注册表） | `EventMesh`, `FlowHandle` | `types-core` |
| `agent-types-core` | 基础类型（冻结） | `Layer<I,O>`, `Uncertain<T>` | 无 |
| `agent-types-ext` | 业务类型 | — | `types-core` |
| `agent-reaction` ★ | 反射短路 | `ReactionRule` | `mesh`, `state` |
| `agent-state` ★ | 短/中/长程 + fork/diff/rollback + MVCC | `AgentState::snapshot/fork/evaluate` | `types-core` |
| `agent-metacognition` ★ | 三信号在线自校准 + TTS | `CalibrationModel`, `TTSSignal` | `mesh`, `state` |
| `agent-persona` ★ | 身份/关系/履历（MVCC） | `Persona::snapshot` | `types-core` |
| `agent-character` ★ | 核心价值观+偏好 | `Character::check_core_values` | `types-core` |
| `agent-session` | 对话域（MVCC + 能力注册表） | `Session::process_turn` | `mesh`, 五维, `types-ext` |
| `agent-task` | 任务域 | `Task::check_ready`, `TaskManifest` | `mesh`, `state`, `types-ext` |
| `agent-collaboration` | 多 Agent 协作 | `Collaboration::delegate` | `mesh`, `state`, `persona`, `crdt` |
| `agent-perception` | 感知域 | `PerceptionPipeline` | `mesh`, `state`, `types-ext` |
| `agent-memory` | 统一记忆 | `VectorStore`, `UnifiedMemory` | `mesh`, `state`, `persona`, `crdt` |
| `agent-reasoning` | 推理域 | `Reasoner`, `ToTExplorer` | `mesh`, `state`, `types-ext`, `uncertainty` |
| `agent-execution` | 执行域（可用 uwu_wasm 沙箱） | `ActionExecutor` | `mesh`, `state`, `types-ext`, `tools` |
| `agent-guard` | 五层闸门 | `InstructionRule`, `BudgetRule`, `EgressRule` | `types-ext` |
| `agent-learning` | 学习触发+Guard博弈 | `LearnCondition`, `SkillTarget` | `mesh`, `state`, `guard` |
| `agent-uncertainty` | 贝叶斯不确定性 | `UncertaintyAggregator` | `types-core` |
| `agent-crdt` | CRDT 状态 | `CRDTStore`, `StateMerger` | `types-core` |
| `agent-tools` | MCP 协议 | `ToolExecutor`, `MCPClient` | `types-ext` |
| `agent-wiki` | 多 Agent 协作知识库（MVCC 版本化 + 语义检索） | `WikiPage`, `WikiRepo`, `MemoryWikiStore` | `types-core` |
| `agent-core` | 会话管理 + FlowGraph(基于 uwu_visual_script) + 能力注册表 | `Agent`, `FlowGraph`, `CapabilityRegistry` | `mesh`, 五维, 所有能力域, `uwu_visual_script` |
| **Sidecar** | | | |
| `agent-sidecar-consolidator` | 独立巩固（LearnNode+Guard） | — | `memory`, `learning`, `guard`, `mesh` |
| `agent-sidecar-monitor` | 独立监控 | — | `mesh` |
| **基础设施** | | | |
| `uwu_visual_script` | 可视化脚本引擎（FlowGraph 执行基础设施） | `Graph`, `NodeDefinition`, `SlotProgram`, `Vm` | `types-core`（概念上独立） |
| `uwu_wasm` | WASM 沙箱引擎（Component Model + WASI p2 + 安全策略） | `Sandbox`, `Policy`, `Loader` | 独立（可被 `agent-execution` 消费） |
| `uwu_database` | 统一数据访问层（SQL + Cache + VectorStore） | `Database`, `VectorStore`, `Repository` | 独立（被 `agent-memory` 消费） |
| `uwu_event_mesh` | 事件网格实现（为 `agent-mesh` 提供底层能力） | `EventMesh`, `FlowHandle`, `TypeRegistry` | 独立 |
| `uwu_logger` | 日志系统 | — | 独立 |

---

### 12.1 agent-wiki —— 多 Agent 协作知识库

> LLM Wiki 是一个多 Agent 协作编辑的结构化知识库。WikiPage 带 MVCC 版本历史，
> WikiRepo 提供可插拔存储后端（当前 MemoryStore，生产接 uwu_database 向量检索）。

```
Wiki 操作流:

  创建页面:  Perception → WikiRepo.save(page)
  编辑页面:  fork(State) → 沙盒推演 → evaluate() → WikiRepo.save(page)
              └─ version += 1，追加 WikiPageVersion 到 history
  版本历史:  WikiPage.version_history → diff_versions(v1, v2) → PageDiff
  语义搜索:  WikiRepo.search(query) → 全文匹配（当前）/ 向量检索（接 uwu_database）
  协作编辑:  agent-collaboration.delegate() + CRDT merge（无冲突合并）
  变更通知:  agent-mesh.publish("agent.wiki.updated", event)
  安全控制:  Character.check_core_values() + GuardLayer（五层闸门）
  知识巩固:  Sidecar-Consolidator 消费 wiki 事件 → LearnNode → 持久化

类型:
  WikiPage { page_id, title, content(md), tags, category, status,
             current_version, version_history: Vec<WikiPageVersion>,
             references, referenced_by, created_by, timestamps }
  WikiPageVersion { version, title, content, edit_summary, edited_by, edited_at }
  PageDiff { title_changed, content_added, content_removed, v1, v2 }

持久化:
  WikiRepo trait（async CRUD + search/tag/category/status 筛选 + list 分页）
  MemoryWikiStore（开发调试用，HashMap 实现）
  → 后续接 uwu_database: VectorStore（语义检索）+ PostgreSQL（结构化查询）
```

---

## 13. 技术栈

```
推理循环:     Reasoning→Acting→Observing（状态管理由 AgentState 替代 ReAct scratchpad）
              + Tree-of-Thought（复杂任务，Beam Search）
反应层:       规则引擎（编译期注册，运行时匹配，省 30-50% token）
状态管理:     AgentState 短/中/长程 + fork()推演沙盒 + JEPA pred_error(EMA)
              + MVCC 并发（主进程写，Sidecar 读快照）
              + CRDT(多Agent共享状态)
元认知评分:   三信号融合 = w1×verifier + w2×(1-pred_error) + w3×cost_remaining
              成本：verifier(~50ms本地模型) + pred_error(零LLM call) + cost(零LLM call)
TTS机制:      渐进式预算压力（Normal→Degraded→Urgent→Abort，主动降推理策略而非硬截断）
记忆存储:     Qdrant(向量) + PostgreSQL(元数据+关联关系)
知识库:       agent-wiki（MVCC 版本化 WikiPage + WikiRepo trait + MemoryStore）
              多 Agent 协作编辑 + 版本历史 + diff + rollback + 语义检索
消息中间件:   NATS / JetStream（持久化事件日志）
              + SerializedEnvelope（跨进程类型安全，类型注册表）
              + EventEnvelope(sequence_number + replay_id + 幂等消费)
              Crash Recovery: JetStream ack + checkpoint 恢复，重放事件标记 replay_id 跳过副作用
工具协议:     MCP(优先) + OpenAI Function Calling(兼容)
验证逻辑:     规则验证器 + 本地模型(Qwen2.5-0.5B) + API模型(gpt-4o-mini)
安全守卫:     GuardLayer 五层闸门（指令/参数/能力/预算/egress，不可自提升）
学习触发:     LearnNode（条件触发 + SkillTarget 本地/远程 + Guard egress 博弈）
              自进化防护：SkillVersion版本化 + 沙箱验证 + 自动回滚 + 人工审批门槛
任务协作:     TaskManifest（AgentCard + DelegationPolicy + SettlementPolicy）
因果分析:     简化SCM（固定变量集 + 历史统计强度）
PII处理:      Presidio(检测) + AES-GCM(可逆加密)
异步运行时:   Tokio
序列化:       Serde(JSON + MessagePack)
可观测性:     tracing + OpenTelemetry
能力加载:     运行时动态注册（CapabilityRegistry + trait object）
图执行引擎:   uwu_visual_script（Graph → ExecutionPlan → SlotProgram → VM）
              + FlowGraph 作为领域包装层（P→M→R→E 管道语义）
              + ExecutionPlan 可跨进程 serde（Sidecar 可独立执行子图）
              + Step budget + Cancel + Middleware 保证安全执行
WASM沙箱:     uwu_wasm（Component Model + WASI Preview 2 + 多沙箱注册表）
              + 零信任能力策略 + 金丝雀发布 + 时间旅行调试
              + eBPF 双重可信链验证
              + agent-execution 可通过 uwu_wasm 沙箱执行不可信代码
数据访问:     uwu_database（SQL + Cache + VectorStore + 多租户 + 迁移）
事件网格实现:  uwu_event_mesh（SerializedEnvelope + TypeRegistry + 四路通道 + 持久化回放）
```

---

## 14. 配置示例

```toml
# uwu_agent_engine.toml

[agent]
name = "uwu_agent_engine"
version = "0.0.1"

[mesh]
main_capacity = 64; consolidation_capacity = 256
monitoring_capacity = 64; system_capacity = 128
type_registry_enabled = true  # 跨进程类型安全

[reaction]
enabled = true
rules = ["popup-close", "rate-limit-retry", "captcha-detect"]

[state]
max_checkpoints = 20; fork_depth_limit = 5
mvcc_enabled = true

[metacognition]
calibration_model = "Qwen2.5-0.5B-Instruct"
confidence_threshold = 0.4; drift_window = 50
[metacognition.weights]
verifier = 0.5; pred_error = 0.3; cost_remaining = 0.2
[metacognition.budget]
max_tokens = 100000; max_time_seconds = 3600; max_retries = 10
[metacognition.tts]
enabled = true
normal_threshold = 0.5
degraded_threshold = 0.2
urgent_threshold = 0.05

[character]
core_values = [
  { name = "privacy_first", enforcement = "HardConstraint" },
  { name = "honesty_first", enforcement = "SoftGuideline" },
]
[character.preferences]
tool_preference = ["web_search", "code_interpreter"]
risk_tolerance = 0.3
uncertainty_strategy = "SearchFirst"
output_style = "StepByStep"

[memory]
vector_backend = "qdrant"; vector_url = "http://localhost:6334"
metadata_backend = "postgres"; metadata_url = "postgres://localhost/uwu_agent_engine"

[guard]
enabled = true; audit_log_path = "/var/log/uwu_agent_engine/guard.log"
rules = ["no-rm-rf", "no-network-to-internal", "file-size-limit", "port-allowlist"]
[guard.budget]
max_tokens = 100000; max_retries = 10; max_time_seconds = 3600
[guard.egress]
mcp_write_allowlist = ["trusted-mcp-1", "trusted-mcp-2"]

[learning]
enabled = true
trigger_conditions = ["significant_error", "new_pattern_detected", "user_confirmed_success"]
skill_extraction_confidence_threshold = 0.85
skill_target_default = "LocalCode"
mcp_skill_write_enabled = false

[execution]
max_parallel_actions = 8; action_timeout_ms = 30000
mcp_server_url = "http://localhost:8080"

[consolidation]
queue_size = 256; max_concurrent = 4; nats_url = "nats://localhost:4222"

[monitoring]
min_interval_secs = 60; anomaly_trigger_only = true
nats_url = "nats://localhost:4222"

[capabilities]
dynamic_loading = true  # 运行时动态注册能力域
registered = ["perception", "memory", "reasoning", "execution"]

[pii]
strategy = "context_aware"; encryption = "AES-GCM"
key_path = "/etc/uwu_agent_engine/pii-key.enc"

[tracing]
enabled = true; export_to_otel = true
```

---

## 15. 实施路线图

> 详细路线图（含逐任务 checkbox、关键文件清单、验收标准、依赖关系图）参见 **[ROADMAP.md](ROADMAP.md)**。

| 阶段 | 内容 | 周期 | 状态 |
|---|---|---|---|
| **✅ 基础设施** | | | |
| 0a | uwu_event_mesh | — | ✅ |
| 0b | uwu_visual_script | — | ✅ |
| 0c | uwu_wasm | — | ✅ |
| 0d | uwu_database | — | ✅ |
| 0e | uwu_logger | — | ✅ |
| **待实施** | | | |
| 1 | 五维：state + reaction + metacognition + persona + character | 2-3 周 | ⬜ |
| 2 | agent-mesh（uwu_event_mesh 的 Agent 语义包装） | 1 周 | ⬜ |
| 3 | 能力域 + FlowGraph(基于 visual_script) + FlowEngine | 2-3 周 | ⬜ |
| 4 | Session 主循环编排 | 1-2 周 | ⬜ |
| 5 | Task + Collaboration | 1-2 周 | ⬜ |
| 6 | LearnNode 自学习 | 1 周 | ⬜ |
| 7 | GuardLayer 五层闸门 | 1 周 | ⬜ |
| 8 | Sidecar（consolidator + monitor） | 1-2 周 | ⬜ |
| 9 | 集成测试 + 性能基准 + TTS 验证 | 1-2 周 | ⬜ |
| **合计** | | **11-17 周** | |

### 关键依赖链

```
State → 五维全部 → Session 主循环 → Task/Collaboration → LearnNode
                                ↘ GuardLayer ↗
```
State 是最底层依赖，建议优先实施。ROADMAP.md 中包含每个阶段的逐文件任务清单。

---
