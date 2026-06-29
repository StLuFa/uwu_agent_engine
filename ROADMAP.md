# uwu_agent_engine 实施路线图

> 本文档是 `ARCHITECTURE.md` 第 15 节的详细展开。
> 每个阶段按可独立交付的增量拆分，标注了关键文件、关键 trait、依赖关系和验收标准。

---

## 目录

1. [总体时间线](#1-总体时间线)
2. [阶段 0：基础设施（✅ 已完成）](#2-阶段-0基础设施-已完成)
3. [阶段 1：Agent 五维](#3-阶段-1agent-五维)
4. [阶段 2：agent-mesh Agent 语义包装](#4-阶段-2agent-mesh-agent-语义包装)
5. [阶段 3：能力域 + FlowGraph + FlowEngine](#5-阶段-3能力域--flowgraph--flowengine)
6. [阶段 4：Session 主循环编排](#6-阶段-4session-主循环编排)
7. [阶段 5：Task + Collaboration](#7-阶段-5task--collaboration)
8. [阶段 6：LearnNode 自学习](#8-阶段-6learnnode-自学习)
9. [阶段 7：GuardLayer 安全守卫](#9-阶段-7guardlayer-安全守卫)
10. [阶段 8：Sidecar 独立进程](#10-阶段-8sidecar-独立进程)
11. [阶段 9：集成测试 + 性能基准](#11-阶段-9集成测试--性能基准)
12. [依赖关系图](#12-依赖关系图)

---

## 1. 总体时间线

```
已 完 成 ─────────────────────────────────────────────────────────────────
  阶段 0a-e  基础设施（5 个 crate）
  阶段 1a    agent-state + agent-types-core（Agent 状态维度）
  阶段 1b    agent-reaction（反射短路维度）
  阶段 1c    agent-metacognition（元认知维度）

待 实 施 ─────────────────────────────────────────────────────────────────
  阶段 1d-e  五维剩余（persona / character）
  阶段 2    ██████████░░░░░░░░░░░░░  agent-mesh 包装（1 周）
  阶段 3    ████████████████████░░░░  能力域 + FlowGraph（2-3 周）
  阶段 4    ██████████████░░░░░░░░░░  Session 主循环（1-2 周）
  阶段 5    ██████████████░░░░░░░░░░  Task + Collaboration（1-2 周）
  阶段 6    ██████████░░░░░░░░░░░░░░  LearnNode（1 周）
  阶段 7    ██████████░░░░░░░░░░░░░░  GuardLayer（1 周）
  阶段 8    ██████████████░░░░░░░░░░  Sidecar（1-2 周）
  阶段 9    ██████████████░░░░░░░░░░  集成测试（1-2 周）
                                   ↑
                              11-17 周
```

---

## 2. 阶段 0：基础设施（✅ 已完成）

### 0a. uwu_event_mesh — 事件网格

| 项目 | 说明 |
|---|---|
| **Crate** | `pkg/uwu_event_mesh` |
| **关键类型** | `EventMesh`, `Envelope`, `SerializedEnvelope`, `TypeRegistry`, `FlowHandle`, `FlowReceiver`, `Subscription` |
| **已完成能力** | 层级 topic pub/sub、因果信封、跨进程序列化信封、类型注册表、四路通道（main/consolidation/monitoring/system）、JSONL 持久化+回放、段式滚动存储、背压策略（Block/DropNewest/DropOldest/Disconnect）、真·DropOldest 环形通道、Consumer Group（RoundRobin/KeyHash）、Ack/Redelivery/DLQ、effectively-once 幂等消费、服务端 Filter、批量 Pull、跨网格 Bridge、CRC 崩溃恢复 |
| **测试覆盖** | topic 匹配、pub/sub、通配、幂等、回放、批量写、索引复用、CRC 崩溃恢复、四种背压、DropOldest、跨网格桥接、时间范围裁剪、段式滚动与重启加载、SerializedEnvelope 往返、TypeRegistry 注册/反序列化/拒绝未知类型、FlowHandle 四路通道/序列号单调/recv_any 多路复用/clone 共享 |

### 0b. uwu_visual_script — 可视化脚本引擎

| 项目 | 说明 |
|---|---|
| **Crate** | `pkg/uwu_visual_script` |
| **关键类型** | `Graph`, `Node`, `Edge`, `Pin`, `NodeDefinition`, `NodeLibrary`, `Purity`, `ExecNext`, `SlotProgram`, `ExecutionPlan`, `Vm`, `HostServices`, `InvokeCtx` |
| **已完成能力** | Graph 模型（节点/边/变量/入口）、Pin 系统（exec/data 分离）、Pure/Impure 节点区分、Graph → ExecutionPlan 编译（类型校验/wildcard/纯子图折叠）、ExecutionPlan → SlotProgram 实例化、SlotProgram 扁平指令 IR（LoadConst/Move/LoadVar/StoreVar/CallPure/CallImpure/Jump）、VM 同步+异步双解释器、Step budget 防护、Cancel 取消、Middleware 钩子（Before/After）、流式 ChunkTx 输出、ExecutionPlan serde（跨进程稳定） |
| **测试覆盖** | 编译往返、类型校验、纯子图优化、VM 同步/异步执行、step budget、cancel、中间件、流式输出 |

### 0c. uwu_wasm — WASM 沙箱引擎

| 项目 | 说明 |
|---|---|
| **Crate** | `pkg/uwu_wasm` |
| **关键类型** | `SandboxEngine`, `Sandbox`, `SandboxRegistry`, `Policy`, `Attestor`, `Loader`, `HotSwap`, `CanaryRouter`, `TimeTravelSession`, `EbpfBridge` |
| **已完成能力** | Component Model + WASI Preview 2、多沙箱注册表（多租户）、零信任能力策略（Policy + ResourceCaps）、执行回执（零知识风格）、eBPF 双重可信链验证、文件加载器 + 热插拔（mtime 轮询 + 原子指针替换）、金丝雀发布框架 + 自愈机制、时间旅行调试（快照/倒带/差分/重放） |

### 0d. uwu_database — 统一数据访问层

| 项目 | 说明 |
|---|---|
| **Crate** | `pkg/uwu_database` |
| **关键类型** | `Database`, `DbPool`, `Cache`, `Repository`, `VectorStore`, `TenantCtx`, `Migrator`, `Features` |
| **已完成能力** | SQL 后端（PostgreSQL/MySQL/SQLite 编译期 feature 切换）、缓存后端（Memory/Redis）、向量后端（Memory/Pgvector/Qdrant/LanceDB）、多租户上下文、社区版/企业版 feature 开关、Repository 泛型 CRUD + 分页、数据库迁移系统 |

### 0e. uwu_logger — 日志系统

| 项目 | 说明 |
|---|---|
| **Crate** | `pkg/uwu_logger` |
| **关键类型** | `Logger`, `LogLevel` |
| **已完成能力** | 基础日志抽象、Println 实现 |

---

## 3. 阶段 1：Agent 五维

> **依赖：** 阶段 0（无需阶段 2+）
> **目标：** agent-state / agent-reaction / agent-metacognition / agent-persona / agent-character 五个 crate 独立可编译、独立可测试

### 3.1 agent-state ✅（已完成）

> **实施日期：** 2026-06-29 |
> **测试结果：** 21 passed, 0 failed, 0 warnings |
> **关联：** agent-types-core（Action/ActionParams/ActionStatus/AgentId/Uncertain/Layer 同步实现）

```
crates/agent-state/
├── Cargo.toml
├── README.md             // 完整使用文档 + 示例
└── src/
    ├── lib.rs              // re-exports ✅
    ├── short.rs            // ShortTermWS + ContextDescriptor + Hypothesis ✅
    ├── mid.rs              // MidTermWS + ActionRecord + Fact + Constraint ✅
    ├── long.rs             // LongTermWS + TaskProgress + BudgetConsumed ✅
    ├── state.rs            // AgentState + StateId + fork/snapshot/apply ✅
    ├── evaluate.rs         // evaluate() → StateScore ✅
    ├── diff.rs             // StateDiff + compute_pred_error + update_pred_error ✅
    ├── checkpoint.rs       // StateCheckpoint + checkpoint/rollback ✅
    ├── mvcc.rs             // StateSnapshot + MVCC versioning ✅
    └── confidence.rs       // ConfidenceMap ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `ShortTermWS` 定义 | P0 | `version`, `current_context`, `last_action`, `last_observation`, `pending_hypotheses` |
| ✅ `MidTermWS` 定义 | P0 | `version`, `action_history`, `known_facts`, `recent_pattern`, `active_constraints` |
| ✅ `LongTermWS` 定义 | P0 | `version`, `task_progress`, `accumulated_pred_error`, `budget_consumed` |
| ✅ `AgentState` 结构体 | P0 | 组合三层 WS + state_id + timestamp + confidence + parent_state_id |
| ✅ `fork()` 实现 | P0 | 完整 clone + 新 state_id + 链 parent_state_id |
| ✅ `apply_action()` 实现 | P0 | short_term.version += 1 + 更新 current_context |
| ✅ `snapshot()` 实现 | P0 | 生成 StateSnapshot + 计算全局版本号 |
| ✅ `apply_hypothetical()` 实现 | P1 | 沙盒推演：action 写入 action_history（标记 Hypothetical） |
| ✅ `evaluate()` 实现 | P0 | 综合评分：事实一致性 + 目标对齐 + 约束满足 |
| ✅ `diff()` 实现 | P0 | 两份 State 差异：facts_added/modified/removed |
| ✅ `compute_pred_error()` 实现 | P0 | JEPA 预测误差：diff 规模 / total_facts |
| ✅ `update_pred_error()` 实现 | P0 | EMA 更新：0.3×err + 0.7×accumulated |
| ✅ `checkpoint()` + `rollback()` | P1 | 序列化当前 State → 回滚 |
| ✅ `InteractionPattern` 检测 | P1 | recent_success_rate < 0.3 连续 5 步 → `is_failure_loop()` / `is_loop_detected()` |
| ✅ 单元测试：fork 不修改原 State | P0 | 21 tests, 0 failed |
| ✅ 单元测试：apply_action 版本号递增 | P0 | |
| ✅ 单元测试：snapshot 版本号 = max(三层) | P0 | |
| ✅ 单元测试：pred_error EMA 收敛 | P0 | |
| ✅ 单元测试：checkpoint → rollback 往返 | P1 | |

**关键 Trait：** 无外部 trait 依赖。AgentState 是纯数据结构 + 方法。

**验收标准（已验证）：**
```bash
cargo test -p agent-state   # 21 passed, 0 failed, 0 warnings
cargo check -p agent-state  # 0 errors, 0 warnings
```

---

### 3.2 agent-reaction ✅（已完成）

> **实施日期：** 2026-06-29 |
> **测试结果：** 22 passed, 0 failed, 0 warnings |
> **关联：** 需 tokio `sync + rt + macros` features

```
crates/agent-reaction/
├── Cargo.toml
├── README.md               // 完整使用文档 + 示例 + 自定义规则指南
└── src/
    ├── lib.rs              // ReactionLayer + ReactionRule trait + Builder ✅
    ├── rules/
    │   ├── mod.rs          // 子模块声明 + re-exports ✅
    │   ├── popup_close.rs  // PopupCloseRule ✅
    │   ├── rate_limit.rs   // RateLimitRetryRule ✅
    │   ├── captcha.rs      // CaptchaDetectRule ✅
    │   └── idle.rs         // IdleTimeoutRule ✅
    └── stats.rs            // ReactionStats (AtomicU64 hits/misses + hit_rate) ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☑ `ReactionRule` trait 定义 | P0 | `fn matches(&self, state: &AgentState) -> bool` + `async fn react(&self, state: &AgentState) -> Action` |
| ☑ `ReactionLayer` 结构体 + `intercept()` | P0 | 顺序遍历 rules，命中则返回 `Reaction::Hit(Action)`，否则 `Reaction::Miss` |
| ☑ `PopupCloseRule` 实现 | P1 | 文本关键词匹配弹窗描述 → 返回 Click 动作 |
| ☑ `RateLimitRetryRule` 实现 | P1 | 文本关键词匹配限流信号 → 返回 Wait+Retry 动作 |
| ☑ `CaptchaDetectRule` 实现 | P2 | 文本关键词匹配验证码 → 返回 RequestHuman 动作 |
| ☑ `IdleTimeoutRule` 实现 | P2 | 检测失败循环或停滞状态 → 返回 ReEvaluateGoal 动作 |
| ☑ `ReactionStats` 结构体 | P1 | AtomicU64 hits/misses + total() + hit_rate() |
| ☑ `ReactionLayerBuilder` | P0 | Builder 模式：`ReactionLayer::builder().add_rule(r1).add_rule(r2).build()` |
| ☑ 单元测试：每个内置规则 match/miss | P1 | 4 规则 × 3-4 场景 = 17 tests |
| ☑ 单元测试：Hit 时短路（不调用后续规则） | P0 | 22 tests, 0 failed |
| ☑ 单元测试：stats 计数正确 | P1 | |
| ⬜ 基准测试：intercept() 延迟 < 1ms（100 rules） | P2 | 延后 |

**依赖：** `agent-state`（读 State），`agent-types-core`（Action 类型）

**关键 Trait（已实现）：**
```rust
#[async_trait]
pub trait ReactionRule: Send + Sync {
    fn matches(&self, state: &AgentState) -> bool;
    async fn react(&self, state: &AgentState) -> Action;
}
```

**验收标准（已验证）：**
```bash
cargo test -p agent-reaction   # 22 passed, 0 failed, 0 warnings
cargo check -p agent-reaction  # 0 errors, 0 warnings
```

---

### 3.3 agent-metacognition ✅（已完成）

> **实施日期：** 2026-06-29 |
> **测试结果：** 16 passed, 0 failed, 0 warnings |
> **关联：** 需 tokio `sync + rt + macros` features

```
crates/agent-metacognition/
├── Cargo.toml
├── README.md               // 完整使用文档 + TTS/三信号融合/异常检测示例
└── src/
    ├── lib.rs              // MetaAction 枚举 + 模块声明 ✅
    ├── evaluate.rs         // Metacognition + evaluate() 三信号融合 ✅
    ├── calibrate.rs        // CalibrationModel trait + CalibrationResult ✅
    ├── tts.rs              // TTSSignal + classify_tts() + compute_cost_remaining ✅
    ├── anomaly.rs          // AnomalyDetector + concept drift 检测 ✅
    └── history.rs          // CalibrationRecord + CalibrationHistory 环形缓冲 ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☑ `MetaScoreWeights` 定义 | P0 | `verifier: 0.5, pred_error: 0.3, cost_remaining: 0.2`（可配置） |
| ☑ `CalibrationModel` trait 定义 | P0 | `async fn calibrate(state, decision_text) -> CalibrationResult`（用 &str 解耦） |
| ☑ `CalibrationResult` 结构体 | P0 | `raw_confidence, calibrated_confidence, should_retry, reasoning` |
| ☑ `MetaAction` 枚举 | P0 | Proceed / RetryDecision / RequestClarification / SwitchStrategy / DelegateToHuman / AbortOnBudget |
| ☑ `evaluate()` 三信号融合 | P0 | `meta_score = w1×verifier + w2×(1-pred_error) + w3×cost_remaining` + InteractionPattern 消费 |
| ☑ `MetacognitiveAssessment` 结构体 | P0 | `calibration, meta_score, knows_unknown, concept_drifting, budget_exhausted, suggested_action` |
| ☑ `compute_cost_remaining()` | P0 | 委托给 BudgetConsumed::cost_remaining_fraction() |
| ☑ `TTSSignal` 枚举 + `tts_signal()` | P0 | Normal/Degraded/Urgent/Abort 四级，classify_tts() 分档 |
| ☑ `calibrate_with_outcome()` | P0 | state.update_pred_error(actual) + 追加 CalibrationRecord + anomaly_detector.update() |
| ☑ `AnomalyDetector` 结构体 | P1 | 滑动窗口（50 条）+ EMA 基线更新 + drift_threshold=0.2 |
| ☑ `CalibrationHistory` 管理 | P1 | VecDeque 环形缓冲，容量 1000，push()/recent(n)/recent_avg_meta_score() |
| ☑ 单元测试：三信号融合公式计算正确 | P0 | 16 tests, 0 failed |
| ☑ 单元测试：TTS 分档边界（0.5/0.2/0.05） | P0 | |
| ☑ 单元测试：loop_detected → SwitchStrategy | P0 | |
| ☑ 单元测试：cost < 0.05 → AbortOnBudget | P0 | |
| ☑ 单元测试：anomaly detector 漂移检测 | P1 | |
| ⬜ 基准测试：evaluate() 延迟 < 100ms（不计 verifier） | P1 | 延后 |

**依赖：** `agent-state`（读 pred_error、recent_pattern、budget_consumed）

**验收标准（已验证）：**
- evaluate() 不接受 LLM call（verifier 由 CalibrationModel trait 注入，测试用 mock）✅
- 三信号中两路是纯计算（pred_error + cost_remaining），延迟 < 1ms ✅
```bash
cargo test -p agent-metacognition   # 16 passed, 0 failed, 0 warnings
cargo check -p agent-metacognition  # 0 errors, 0 warnings
```

---

### 3.4 agent-persona（1-2 天）

```
crates/agent-persona/
├── Cargo.toml
└── src/
    ├── lib.rs              // Persona + PersonaSnapshot + PersonaContext
    ├── identity.rs         // Identity（名称/角色/组织/背景）
    ├── relationships.rs    // RelationshipGraph（AgentId → 关系类型/信任度/协作历史）
    └── history.rs          // PersonaHistory（关键经历的序列化日志）
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `Identity` 结构体 | P0 | `name, role, organization, background, expertise: Vec<String>` |
| ☐ `RelationshipGraph` 结构体 | P0 | 有向图：AgentId → `Relationship { trust: f32, type: RelationType, collaboration_count: u32 }` |
| ☐ `PersonaHistory` 结构体 | P1 | `Vec<PersonaEvent>` — 关键经历的序列化日志 |
| ☐ `Persona` 结构体 | P0 | `version: u64, identity, relationships, history` |
| ☐ `to_context_injection()` 实现 | P0 | 生成可注入推理上下文的 `PersonaContext` 字符串 |
| ☐ `update_relationship()` 实现 | P0 | 根据协作结果更新关系图，version += 1 |
| ☐ `snapshot()` 实现 | P0 | 生成 `PersonaSnapshot` 供 Sidecar 读取 |
| ☐ MVCC：版本号管理 | P1 | 主进程写入 version += 1，Sidecar 读快照 |
| ☐ 单元测试：关系更新 + 版本号变更 | P0 | |
| ☐ 单元测试：snapshot 不阻塞写入 | P1 | |

**依赖：** `agent-types-core`（AgentId, CollaborationOutcome）

---

### 3.5 agent-character（1-2 天）

```
crates/agent-character/
├── Cargo.toml
└── src/
    ├── lib.rs              // Character + CoreValue + Preferences
    ├── values.rs           // CoreValue + ValueEnforcement + check_core_values()
    └── preferences.rs      // Preferences + UncertaintyStrategy + OutputStyle
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `CoreValue` 结构体 | P0 | `name, description, enforcement: HardConstraint / SoftGuideline` |
| ☐ `Preferences` 结构体 | P0 | `tool_preference, risk_tolerance, uncertainty_strategy, output_style` |
| ☐ `Character` 结构体 | P0 | `core_values: Vec<CoreValue>, preferences: Preferences` |
| ☐ `check_core_values()` 实现 | P0 | 遍历 core_values，HardConstraint 违反 → `Err(ValueViolation)` |
| ☐ `to_context_injection()` 实现 | P0 | 生成偏好注入字符串（不确定策略/输出风格） |
| ☐ 内置 CoreValue 预设 | P1 | `privacy_first`, `honesty_first`, `no_destructive_actions` |
| ☐ `UncertaintyStrategy` 枚举 | P0 | `SearchFirst, AskUserFirst, BestGuessAndConfirm` |
| ☐ `OutputStyle` 枚举 | P0 | `Concise, Detailed, StepByStep` |
| ☐ 单元测试：HardConstraint 违反检测 | P0 | |
| ☐ 单元测试：SoftGuideline 不阻断 | P1 | |

**依赖：** `agent-types-core`（Action, ValueViolation）

**关键约束：** Character.core_values 不可变（构造后不提供 setter）。

---

## 4. 阶段 2：agent-mesh Agent 语义包装

> **依赖：** 阶段 0a（uwu_event_mesh）、阶段 1（五维）
> **目标：** 在 uwu_event_mesh 之上建立 Agent 领域的事件类型与 topic 约定

```
crates/agent-mesh/
├── Cargo.toml
└── src/
    ├── lib.rs              // re-exports
    ├── topics.rs           // Agent 领域 topic 常量（state.snapshot / task.created / decision.made / ...）
    ├── events/
    │   ├── mod.rs
    │   ├── state.rs        // StateSnapshotEvent（封装 StateSnapshot）
    │   ├── task.rs         // TaskCreated / TaskCompleted / SubtaskDelegated
    │   ├── decision.rs     // DecisionMade / DecisionRetried
    │   └── persona.rs      // PersonaUpdated / RelationshipChanged
    └── registry.rs         // AgentTypeRegistry（预注册所有 Agent 事件类型）
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ 定义 topic 命名空间常量 | P0 | `TOPIC_STATE = "agent.state.>"`, `TOPIC_TASK = "agent.task.>"`, `TOPIC_DECISION = "agent.decision.>"`, `TOPIC_PERSONA = "agent.persona.>"` |
| ☐ `StateSnapshotEvent` 封装 | P0 | 包装 `StateSnapshot` + topic 约定 `"agent.state.snapshot"` |
| ☐ `TaskCreated` / `TaskCompleted` 事件 | P0 | 封装 Task 生命周期 |
| ☐ `SubtaskDelegated` / `DelegationResult` 事件 | P1 | 封装协作委派 |
| ☐ `DecisionMade` / `DecisionRetried` 事件 | P0 | 封装元认知决策 |
| ☐ `PersonaUpdated` / `RelationshipChanged` 事件 | P1 | 封装 Persona 变更 |
| ☐ `AgentTypeRegistry` 初始化 | P0 | 启动期一次性注册所有 Agent 事件类型到 TypeRegistry |
| ☐ `AgentMesh` 门面 | P0 | 包装 `EventMesh` + `FlowHandle`，提供 agent 语义的 publish 方法 |
| ☐ 单元测试：每种事件序列化/反序列化往返 | P0 | |
| ☐ 单元测试：TypeRegistry 拒绝未知事件 | P0 | |

**关键设计：** 本 crate 是对 `uwu_event_mesh` 的薄包装，不重复实现任何底层机制。只定义 Agent 领域的 topic 命名空间和事件类型。

---

## 5. 阶段 3：能力域 + FlowGraph + FlowEngine

> **依赖：** 阶段 0b（uwu_visual_script）、阶段 1（agent-state）、阶段 2（agent-mesh）
> **目标：** Perception/Memory/Reasoning/Execution 作为 visual_script NodeDefinition，FlowGraph 作为领域包装，FlowEngine 作为主循环执行器

### 5.1 agent-perception（2-3 天）

```
crates/agent-perception/
├── Cargo.toml
└── src/
    ├── lib.rs              // PerceptionPipeline
    ├── parsers/
    │   ├── mod.rs
    │   ├── text.rs         // 文本解析
    │   ├── json.rs         // JSON 结构化解析
    │   └── multimodal.rs   // 多模态占位（图像/音频 → 文本描述）
    ├── pii.rs              // PII 检测（Presidio） + 可逆加密（AES-GCM）
    └── context.rs          // ContextDescriptor 构建
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `Perceiver` trait 定义 | P0 | `async fn perceive(input: RawInput) -> ContextDescriptor` |
| ☐ `PerceptionPipeline` 结构体 | P0 | 解析链：RawInput → ParsedInput → PII scan → ContextDescriptor |
| ☐ 文本解析器（TextParser） | P0 | |
| ☐ JSON 结构化解析器 | P0 | |
| ☐ PII 检测集成 | P1 | Presidio 检测 + AES-GCM 可逆加密 |
| ☐ 注册为 visual_script NodeDefinition | P0 | `"perception.observe"`：Impure + Async |
| ☐ 单元测试：文本解析 + PII 遮蔽 | P0 | |
| ☐ 单元测试：作为 visual_script 节点执行 | P0 | |

### 5.2 agent-memory（2-3 天）

```
crates/agent-memory/
├── Cargo.toml
└── src/
    ├── lib.rs              // re-exports
    ├── unified.rs          // UnifiedMemory（向量 + 元数据）
    ├── retrieve.rs         // retrieve() / retrieve_typed()
    ├── consolidate.rs      // consolidate() — 将 Episode 持久化
    ├── types.rs            // MemoryType 枚举 + Memory + MemoryScore
    └── embedding.rs        // Embedding 生成（调用外部 embedding 服务）
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `VectorStore` trait 定义 | P0 | `search(query, opts) -> Vec<Memory>`, `upsert(id, embedding, metadata)` |
| ☐ `UnifiedMemory` 结构体 | P0 | `vector_db: Arc<dyn VectorStore>, metadata_db: PgPool` |
| ☐ `MemoryType` 枚举 | P0 | `Episodic, Semantic, Procedural, Working` |
| ☐ `Memory` 结构体 | P0 | `id, memory_type, content, embedding, score, state_snapshot` |
| ☐ `retrieve(intent)` — 默认检索 | P0 | 基于意图自动选择检索策略，覆盖 80% 场景 |
| ☐ `retrieve_typed(intent, types)` — 按类型检索 | P1 | 按需降级：仅检索指定 MemoryType |
| ☐ `persist_state(snapshot)` | P0 | StateSnapshot → 向量 embedding + 元数据记录 |
| ☐ `persist_persona(snapshot)` | P1 | PersonaSnapshot → 元数据记录 |
| ☐ `consolidate(episode)` | P0 | Episode → 提取关键记忆 → 写入向量 DB + 元数据 DB |
| ☐ 注册为 visual_script NodeDefinition | P0 | `"memory.retrieve"`：Impure + Async |
| ☐ 单元测试：retrieve 返回相关记忆 | P0 | |
| ☐ 单元测试：persist_state → retrieve 往返 | P0 | |

### 5.3 agent-reasoning（3-4 天）

```
crates/agent-reasoning/
├── Cargo.toml
└── src/
    ├── lib.rs              // re-exports
    ├── reasoner.rs         // Reasoner trait
    ├── tot.rs              // Tree-of-Thought + beam search
    ├── sandbox.rs          // fork() 沙盒推演多个候选动作
    └── strategies.rs       // 推理策略（可根据 TTS 信号切换）
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `Reasoner` trait 定义 | P0 | `async fn reason(state, context) -> Decision` |
| ☐ `Decision` 结构体 | P0 | `candidates: Vec<Action>, scores: Vec<StateScore>, reasoning: String` |
| ☐ `ToTExplorer` 结构体 | P1 | Beam Search：生成候选 → fork 推演 → 评分 → 剪枝 |
| ☐ fork 沙盒推演 | P0 | 对每个候选 action：`state.fork() → apply_hypothetical(action) → evaluate()` |
| ☐ 推理策略切换 | P0 | 根据 TTSSignal：Normal→ToT / Degraded→单步 / Urgent→直接回答 |
| ☐ 注册为 visual_script NodeDefinition | P0 | `"reasoning.decide"`：Impure + Async |
| ☐ 单元测试：单步推理 | P0 | |
| ☐ 单元测试：ToT beam search | P1 | |
| ☐ 单元测试：TTS 信号 → 策略切换 | P0 | |

### 5.4 agent-execution（2-3 天）

```
crates/agent-execution/
├── Cargo.toml
└── src/
    ├── lib.rs              // ActionExecutor
    ├── mcp.rs              // MCP 工具调用
    ├── wasm_sandbox.rs     // uwu_wasm 沙箱执行不可信代码
    └── output.rs           // 输出格式化
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `Executor` trait 定义 | P0 | `async fn execute(action: Action) -> ExecutionResult` |
| ☐ `ActionExecutor` 结构体 | P0 | 调用链：Guard 检查 → MCP 工具 / WASM 沙箱 → 收集结果 |
| ☐ MCP 工具调用 | P0 | HTTP/gRPC 调用 MCP Server |
| ☐ WASM 沙箱执行 | P2 | 通过 uwu_wasm 安全执行不可信代码 |
| ☐ 注册为 visual_script NodeDefinition | P0 | `"execution.act"`：Impure + Async |
| ☐ 单元测试：MCP 工具调用 mock | P0 | |
| ☐ 单元测试：WASM 沙箱执行（可选） | P2 | |

### 5.5 FlowGraph + FlowEngine（2-3 天）

```
crates/agent-core/src/
├── flow.rs               // FlowGraph（领域包装层）
├── engine.rs             // FlowEngine（主循环执行器）
└── capability.rs         // CapabilityRegistry（动态注册）
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `FlowGraph` 结构体 | P0 | 包装 `visual_script::Graph` + 缓存 `SlotProgram` |
| ☐ `FlowGraph::standard()` | P0 | 构建标准 P→M→R→E 管道 |
| ☐ `FlowGraph::high_security()` | P1 | 标准管道 + reasoning.decision → reasoning.validate 回边 |
| ☐ `FlowGraph::from_config()` | P1 | 从 FlowConfig 构建自定义图 |
| ☐ `add_edge_dynamic()` | P0 | 克隆 Graph → 添加边 → 重新编译 → 原子替换 program |
| ☐ `CapabilityRegistry` 结构体 | P0 | `perceivers: Vec<Box<dyn Perceiver>>, reasoners, executors` |
| ☐ `FlowEngine` 结构体 | P0 | `run(flow: &FlowGraph, state: &AgentState) -> Decision` |
| ☐ FlowEngine 与 visual_script VM 集成 | P0 | 使用 Vm::run_entry_async 执行 SlotProgram |
| ☐ 单元测试：standard() 编译成功 | P0 | |
| ☐ 单元测试：add_edge_dynamic() 后 program 更新 | P0 | |
| ☐ 单元测试：FlowEngine 完整执行 P→M→R→E | P0 | |

---

## 6. 阶段 4：Session 主循环编排

> **依赖：** 阶段 1-3 全部
> **目标：** Session 持有五维 + 能力注册表，实现完整的 process_turn 主循环

```
crates/agent-session/
├── Cargo.toml
└── src/
    ├── lib.rs              // Session + process_turn()
    ├── turn.rs             // ConversationTurn
    ├── intent.rs           // IntentTracker
    ├── history.rs          // 对话历史管理
    └── snapshot.rs         // emit_snapshot() → 发给 Sidecar
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `Session` 结构体 | P0 | 持有五维 + CapabilityRegistry + checkpoint 栈 + 对话历史 |
| ☐ `process_turn()` 完整主循环 | P0 | 1.Reaction.intercept → 2.FlowGraph → 3.Metacognition.evaluate → 4.MetaAction 分支处理 → 5.Execution+Guard → 6.Metacognition.calibrate |
| ☐ `enrich_input()` 实现 | P1 | 注入 Persona.context + Character.context |
| ☐ `execute_reaction()` 实现 | P0 | Reaction Hit → 直接执行 → 更新 State |
| ☐ MetaAction 分支处理 | P0 | Proceed / RetryDecision（rollback+重推理）/ RequestClarification（暂停）/ SwitchStrategy（切换推理模式）/ AbortOnBudget |
| ☐ `execute_and_update()` 实现 | P0 | Guard 检查 → 执行 → 用结果修正 State |
| ☐ `emit_snapshot()` 实现 | P0 | 生成 StateSnapshot + PersonaSnapshot → SerializedEnvelope → publish 到 agent-mesh |
| ☐ `IntentTracker` 实现 | P1 | 跟踪用户意图跨 turn 变化 |
| ☐ `checkpoint` 管理 | P1 | 外部副作用前自动 checkpoint |
| ☐ 单元测试：Reaction Hit 短路 | P0 | |
| ☐ 单元测试：RetryDecision → rollback + 重推理 | P0 | |
| ☐ 单元测试：AbortOnBudget 终止 | P0 | |
| ☐ 单元测试：完整 process_turn（mock 所有五维） | P0 | |
| ☐ 集成测试：Session + 真实 FlowGraph + Memory | P1 | |

---

## 7. 阶段 5：Task + Collaboration

> **依赖：** 阶段 4（Session）
> **目标：** 持久任务管理 + 多 Agent 协作委派

### 7.1 agent-task（2-3 天）

```
crates/agent-task/
├── Cargo.toml
└── src/
    ├── lib.rs              // Task + SubtaskDag + Subtask
    ├── manifest.rs         // TaskManifest + AgentCard + SettlementPolicy
    ├── delegation.rs       // DelegationPolicy + DiscoveryStrategy + FallbackStrategy
    ├── settlement.rs       // SettlementPolicy + SettlementMode
    └── scheduler.rs        // Subtask 调度器
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `Task` 结构体 | P0 | `task_id, goal, status, subtask_dag, max_retries, manifest` |
| ☐ `SubtaskDag` 结构体 | P0 | DAG 拓扑：nodes + edges，支持并行/依赖 |
| ☐ `Subtask` 结构体 | P0 | `id, description, status, assigned_agent, flow_graph, max_retries, timeout` |
| ☐ `AgentCard` 结构体 | P0 | `agent_id, name, capabilities, role, priority, endpoint` |
| ☐ `DelegationPolicy` 结构体 | P0 | `discovery: ExactCapability/LoadBalanced/TrustRanked/Auction` |
| ☐ `SettlementPolicy` 结构体 | P0 | `mode: Free/FixedPrice/Metered/Auction` |
| ☐ `check_ready()` 实现 | P0 | 检查 DAG 中可执行的 subtask |
| ☐ `update_progress()` 实现 | P0 | 根据完成的 subtask 更新 State |
| ☐ 单元测试：DAG 调度正确 | P0 | |
| ☐ 单元测试：SettlementPolicy 计费计算 | P1 | |

### 7.2 agent-collaboration（2-3 天）

```
crates/agent-collaboration/
├── Cargo.toml
└── src/
    ├── lib.rs              // Collaboration
    ├── registry.rs         // AgentRegistry + AgentDescriptor
    ├── delegate.rs         // delegate() + DelegationState
    └── negotiate.rs        // negotiate() 协商
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `AgentRegistry` 结构体 | P0 | `agents: HashMap<AgentId, AgentDescriptor>` + capability_index |
| ☐ `Collaboration` 结构体 | P0 | `registry + mesh + pending: DashMap<DelegationId, DelegationState>` |
| ☐ `delegate()` 实现 | P0 | 根据 DelegationPolicy 选择 Agent → 发送 subtask → 等待结果 |
| ☐ `on_delegation_complete()` 实现 | P0 | 接收结果 → 应用 state_delta → 更新 Persona 关系 |
| ☐ `negotiate()` 实现 | P1 | CRDT 状态合并协商 |
| ☐ 单元测试：ExactCapability 匹配 | P0 | |
| ☐ 单元测试：委派 → 完成 → state_delta 合并 | P0 | |

---

## 8. 阶段 6：LearnNode 自学习

> **依赖：** 阶段 4（Session）、阶段 5（Task）
> **目标：** Episode 完成后触发学习，根据条件决定是否提取 Skill

```
crates/agent-learning/
├── Cargo.toml
└── src/
    ├── lib.rs              // re-exports
    ├── trigger.rs          // LearnTrigger + LearnCondition trait
    ├── conditions/
    │   ├── mod.rs
    │   ├── significant_error.rs
    │   ├── new_pattern.rs
    │   └── user_confirmed.rs
    ├── skill.rs            // SkillTarget + SkillVersion
    └── sandbox.rs          // 沙箱验证新 Skill
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `LearnCondition` trait 定义 | P0 | `async fn should_learn(episode, state) -> LearnDecision` |
| ☐ `LearnDecision` 枚举 | P0 | `Skip, ConsolidateEpisode, ExtractSkill { skill_name, target, confidence }, UpdatePreference` |
| ☐ `LearnTrigger` 结构体 | P0 | `conditions: Vec<Box<dyn LearnCondition>>` |
| ☐ `SkillTarget` 枚举 | P0 | `LocalCode { crate_name }, McpRemote { server_id, tool_name, endpoint }` |
| ☐ `SkillVersion` 结构体 | P0 | `version_id, skill_name, target, hash, verified, active` |
| ☐ `SignificantErrorCondition` | P0 | 预测误差 > 阈值 → ExtractSkill |
| ☐ `NewPatternCondition` | P1 | 检测到新模式 → ExtractSkill |
| ☐ `UserConfirmedCondition` | P1 | 用户确认成功 → ExtractSkill |
| ☐ Guard egress 集成 | P0 | ExtractSkill 写入前：check_egress(target) |
| ☐ 沙箱验证新 Skill | P0 | fork() State 沙盒中运行 → 通过后 mark verified |
| ☐ 回滚机制 | P1 | Guard 检测异常 → 自动回滚至上一 SkillVersion |
| ☐ 单元测试：SignificantError 触发学习 | P0 | |
| ☐ 单元测试：McpRemote 需要 Guard egress 通过 | P0 | |
| ☐ 单元测试：SkillVersion 沙箱验证 | P0 | |
| ☐ 单元测试：回滚至上一版本 | P1 | |

---

## 9. 阶段 7：GuardLayer 安全守卫

> **依赖：** 阶段 3（agent-execution）、阶段 6（agent-learning）
> **目标：** 五层硬闸门，编译期注册，不可自提升

```
crates/agent-guard/
├── Cargo.toml
└── src/
    ├── lib.rs              // GuardLayer + GuardBuilder
    ├── rules/
    │   ├── mod.rs
    │   ├── instruction.rs  // InstructionRule + 内置规则
    │   ├── parameter.rs    // ParameterRule
    │   ├── capability.rs   // CapabilityRule
    │   ├── budget.rs       // BudgetRule + TokenBudgetRule
    │   └── egress.rs       // EgressRule + McpWriteAllowlistRule
    └── audit.rs            // AuditLog
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ `InstructionRule` trait | P0 | `async fn check(action) -> Option<GuardViolation>` |
| ☐ `ParameterRule` trait | P0 | `async fn check(action, params) -> Option<GuardViolation>` |
| ☐ `CapabilityRule` trait | P0 | `async fn check(action, context) -> Option<GuardViolation>` |
| ☐ `BudgetRule` trait | P0 | `async fn check(budget_consumed, limits) -> Option<GuardViolation>` |
| ☐ `EgressRule` trait | P0 | `async fn check_egress(target, context) -> Option<GuardViolation>` |
| ☐ `GuardLayer` 结构体 + `enforce()` | P0 | 顺序五层检查 → 返回 allowed actions 或 blocked violations |
| ☐ `GuardBuilder` | P0 | Builder 模式：编译期注册规则 |
| ☐ 内置规则：`NoRmRfRule` | P0 | 禁止递归删除 |
| ☐ 内置规则：`TokenBudgetRule` | P0 | Token 预算检查 |
| ☐ 内置规则：`McpWriteAllowlistRule` | P0 | MCP 写入白名单 |
| ☐ 内置规则：`NoNetworkToInternal` | P1 | 禁止访问内网地址 |
| ☐ 内置规则：`FileSizeLimit` | P1 | 文件操作大小限制 |
| ☐ 内置规则：`PortAllowlist` | P1 | 端口白名单 |
| ☐ `AuditLog` 结构体 | P0 | 记录所有 Guard 事件（hit/block/bypass） |
| ☐ 单元测试：rm -rf 被阻断 | P0 | |
| ☐ 单元测试：Token 耗尽 → Warning | P0 | |
| ☐ 单元测试：未授权 MCP 写入被阻断 | P0 | |
| ☐ 单元测试：所有规则通过 → 放行 | P0 | |
| ☐ 单元测试：enforce() 部分通过 → 返回 allowed + blocked | P0 | |

**关键约束：** GuardLayer 构造后不可修改规则集。Agent 无法在运行时绕过或提升自己的权限。

---

## 10. 阶段 8：Sidecar 独立进程

> **依赖：** 阶段 2（agent-mesh）、阶段 6（agent-learning）、阶段 7（GuardLayer）
> **目标：** Consolidator + Monitor 作为独立进程运行

### 10.1 agent-sidecar-consolidator（2-3 天）

```
crates/agent-sidecar-consolidator/
├── Cargo.toml
└── src/
    └── main.rs             // 独立二进制：消费 consolidation 通道 → LearnNode 触发 → Guard 博弈 → 持久化
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ NATS 连接 + JetStream 订阅 | P0 | `uwu_agent_engine.events.completions` |
| ☐ 反序列化 Episode（TypeRegistry 校验） | P0 | |
| ☐ LearnTrigger 评估 | P0 | |
| ☐ Guard egress 博弈 | P0 | McpRemote → Guard.check_egress() 通过才写入 |
| ☐ UnifiedMemory 持久化 | P0 | consolidate(episode) |
| ☐ 优雅关闭 | P1 | 消费完队列中剩余事件后退出 |
| ☐ 集成测试：Episode → Learn → Persist 端到端 | P0 | |

### 10.2 agent-sidecar-monitor（2-3 天）

```
crates/agent-sidecar-monitor/
├── Cargo.toml
└── src/
    └── main.rs             // 独立二进制：消费 monitoring 通道 → 异常检测 → MetacognitiveReport
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ☐ NATS 连接 + JetStream 订阅 | P0 | `uwu_agent_engine.metrics.>` |
| ☐ 异常检测引擎 | P0 | Metacognition 漂移检测 + State 异常模式 |
| ☐ `MetacognitiveReport` 生成 | P0 | 定期生成报告（每 N 分钟或触发异常时） |
| ☐ 告警输出 | P1 | 日志 / OpenTelemetry / Webhook |
| ☐ 集成测试：异常事件 → 报告生成 | P0 | |

---

## 11. 阶段 9：集成测试 + 性能基准

> **依赖：** 所有阶段
> **目标：** 端到端验证 + 性能基准建立 + TTS 机制验证

### 11.1 集成测试（3-5 天）

| 测试场景 | 说明 |
|---|---|
| ☐ **决策主循环 E2E** | 完整 process_turn：Reaction → FlowGraph → Metacognition → Execution → Calibration |
| ☐ **Reaction 短路 E2E** | 弹窗关闭等高频场景命中 → 0 token 消耗验证 |
| ☐ **Metacognition Retry E2E** | 低信心决策 → 自动回滚 + 重新推理 |
| ☐ **TTS 降级 E2E** | 预算 50% → ToT 禁用；< 5% → Abort |
| ☐ **Session 多轮对话 E2E** | 10 轮连续 turn → State 正确演进 |
| ☐ **Task 多 Subtask E2E** | TaskManifest + DAG 调度 + Completion |
| ☐ **Collaboration 委派 E2E** | 跨 Agent 委派 + Settlement 结算 |
| ☐ **LearnNode 学习 E2E** | Episode 完成 → 触发学习 → Skill 提取 → 沙箱验证 |
| ☐ **GuardLayer 防护 E2E** | 危险命令被拦截 + AuditLog 记录 |
| ☐ **Sidecar 跨进程 E2E** | 主进程 publish → Sidecar 消费 → 持久化/监控 |
| ☐ **Crash Recovery E2E** | 模拟崩溃 → 从 checkpoint 恢复 → 重放事件跳过副作用 |
| ☐ **类型安全 E2E** | 未知类型事件被 TypeRegistry 拒绝 |

### 11.2 性能基准（2-3 天）

| 基准 | 目标 | 说明 |
|---|---|---|
| ☐ `Reaction.intercept()` 延迟 | < 1ms (100 rules) | 规则匹配不随规模退化 |
| ☐ `Metacognition.evaluate()` 延迟 | < 100ms（不计 verifier） | pred_error + cost 纯计算 |
| ☐ `State.fork()` 延迟 | < 1ms（1MB State） | fork() 内存分配 |
| ☐ `State.evaluate()` 延迟 | < 5ms | 状态评分 |
| ☐ `FlowGraph` 编译延迟 | < 50ms | Graph → SlotProgram |
| ☐ `process_turn()` 端到端延迟 | < 3s（不计 LLM call） | 不包括 LLM 推理时间 |
| ☐ `agent-mesh` 吞吐 | > 10k msg/s（单进程） | EventMesh publish + fan-out |
| ☐ 内存占用基线 | < 50MB（空闲 Session） | 不含模型权重 |

### 11.3 TTS 验证（1-2 天）

| 验证项 | 说明 |
|---|---|
| ☐ Normal→Degraded→Urgent→Abort 状态迁移 | 模拟预算消耗 |
| ☐ Degraded 禁用 ToT | 验证推理策略切换 |
| ☐ Urgent 禁止新工具调用 | 验证能力限制 |
| ☐ Abort 优雅终止 | 不丢 checkpoint |

---

## 12. 依赖关系图

```
                    阶段 0 (已完成)
                    ├── uwu_event_mesh ──────────────────────┐
                    ├── uwu_visual_script ──────────────────┐ │
                    ├── uwu_wasm ───────────────────────────┐│ │
                    ├── uwu_database ─────────────────────┐ ││ │
                    └── uwu_logger                        │ ││ │
                                                          │ ││ │
                    阶段 1 (五维)                          │ ││ │
                    ├── agent-state ──────────────────────┤ ││ │
                    ├── agent-reaction ──────┐            │ ││ │
                    ├── agent-metacognition ─┤            │ ││ │
                    ├── agent-persona ───────┤            │ ││ │
                    └── agent-character ─────┤            │ ││ │
                                             │            │ ││ │
                    阶段 2                  │            │ ││ │
                    └── agent-mesh ←────────┴─ event_mesh┘ ││ │
                                            依赖           ││ │
                    阶段 3                                ││ │
                    ├── agent-perception ←──── state      ││ │
                    ├── agent-memory ←──────── state      ││ │
                    ├── agent-reasoning ←───── state      ││ │
                    ├── agent-execution ←───── state ─────┼─┼┘(uwu_wasm可选)
                    └── FlowGraph/FlowEngine ← visual_script┘ │
                                                              │
                    阶段 4                                   │
                    └── agent-session ← 五维 + 能力域 + mesh  │
                                                              │
                    阶段 5                                   │
                    ├── agent-task ← state + session          │
                    └── agent-collaboration ← task + persona  │
                                                              │
                    阶段 6                                   │
                    └── agent-learning ← state + guard        │
                                                              │
                    阶段 7                                   │
                    └── agent-guard ← types-ext               │
                                                              │
                    阶段 8                                   │
                    ├── sidecar-consolidator ← learning+guard+m→sh
                    └── sidecar-monitor ← mesh                │
                                                              │
                    阶段 9                                   │
                    └── 集成测试 + 性能基准 ← 全部             │
```

---

> **下一优先事项：** 启动阶段 1 — `agent-state` crate。
> State 是整个架构的根基，所有其他模块都依赖它。建议从 `AgentState` + `fork()` + `snapshot()` + `evaluate()` 的核心路径开始，快速出一个可编译可测试的 MVP。
