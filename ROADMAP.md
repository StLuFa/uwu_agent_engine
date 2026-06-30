# uwu_agent_engine 实施路线图

> 本文档是 `ARCHITECTURE.md` 第 15 节的详细展开。
> 每个阶段按可独立交付的增量拆分，标注了关键文件、关键 trait、依赖关系和验收标准。

---

## 目录

1. [总体时间线](#1-总体时间线)
2. [阶段 0：基础设施（✅ 已完成）](#2-阶段-0基础设施-已完成)
3. [阶段 1：Agent 五维（✅ 已完成）](#3-阶段-1agent-五维)
4. [阶段 2：agent-mesh（✅ 已完成）](#4-阶段-2agent-mesh-agent-语义包装)
5. [阶段 3：能力域 + FlowGraph（✅ 已完成）](#5-阶段-3能力域--flowgraph--flowengine)
6. [阶段 4：Session 主循环（✅ 已完成）](#6-阶段-4session-主循环编排)
7. [阶段 5：Task + Collaboration（✅ 已完成）](#7-阶段-5task--collaboration)
8. [阶段 6：LearnNode（✅ 已完成）](#8-阶段-6learnnode-自学习)
9. [阶段 7：GuardLayer（✅ 已完成）](#9-阶段-7guardlayer-安全守卫)
10. [阶段 8：Sidecar（✅ 已完成）](#10-阶段-8sidecar-独立进程)
11. [阶段 9：集成测试 + 性能基准](#11-阶段-9集成测试--性能基准)
12. [阶段 W：agent-wiki（✅ 已完成）](#w-agent-wiki)
13. [依赖关系图](#13-依赖关系图)

---

## 1. 总体时间线

```
已 完 成 ─────────────────────────────────────────────────────────────────
  阶段 0a-e  基础设施（5 个 crate）
  阶段 1a    agent-state + agent-types-core（Agent 状态维度）
  阶段 1b    agent-reaction（反射短路维度）
  阶段 1c    agent-metacognition（元认知维度）
  阶段 1d    agent-persona（人物角色维度）
  阶段 1e    agent-character（人格维度）
  阶段 2     agent-mesh（Agent 语义事件网格）
  阶段 3a    agent-perception（感知域）
  阶段 3b    agent-memory（统一记忆）
  阶段 3c    agent-reasoning（推理域）
  阶段 3d    agent-execution（执行域）
  阶段 3e    agent-core（FlowGraph + FlowEngine）
  阶段 4     agent-session（Session 主循环）
  阶段 5a    agent-task（任务域）
  阶段 5b    agent-collaboration（多 Agent 协作）
  阶段 6     agent-learning（LearnNode 自学习）
  阶段 7     agent-guard（GuardLayer 五层闸门）
  阶段 8a    agent-sidecar-consolidator（独立巩固进程）
  阶段 8b    agent-sidecar-monitor（独立监控进程）
  阶段 W     agent-wiki（多 Agent 协作知识库）

已 完 成 ─────────────────────────────────────────────────────────────────
  ✅ P0 缺陷修复（7 crash 点消除）
  ✅ P1 缺陷修复（5 正确性修复）
  ✅ P2 缺陷修复（3 代码质量）
  ✅ 结构性断裂修复（mesh/core/task+collab → session）
  ✅ uwu_database → agent-memory 集成
  ✅ uwu_nats_bridge 新建（NATS/JetStream 跨进程）
  ✅ agent-crdt 完整实现（18 tests）
  ✅ agent-uncertainty 贝叶斯推理（16 tests）
  ✅ crdt→collab, uncertainty→metacog 接入
  ✅ wiki→session+collab+database 三路径接入
  ✅ agent-state-short/mid/long 删除（合并回 agent-state）

待 实 施 ─────────────────────────────────────────────────────────────────
  阶段 9    集成测试 + 性能基准（1-2 周）
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

> **测试结果：** 21 passed, 0 failed, 0 warnings |
> **关联：** agent-types-core（Action/ActionParams/ActionStatus/AgentId/Uncertain/Layer 同步实现，补 0→22 tests）

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
| ✅ `ReactionRule` trait 定义 | P0 | `fn matches(&self, state: &AgentState) -> bool` + `async fn react(&self, state: &AgentState) -> Action` |
| ✅ `ReactionLayer` 结构体 + `intercept()` | P0 | 顺序遍历 rules，命中则返回 `Reaction::Hit(Action)`，否则 `Reaction::Miss` |
| ✅ `PopupCloseRule` 实现 | P1 | 文本关键词匹配弹窗描述 → 返回 Click 动作 |
| ✅ `RateLimitRetryRule` 实现 | P1 | 文本关键词匹配限流信号 → 返回 Wait+Retry 动作 |
| ✅ `CaptchaDetectRule` 实现 | P2 | 文本关键词匹配验证码 → 返回 RequestHuman 动作 |
| ✅ `IdleTimeoutRule` 实现 | P2 | 检测失败循环或停滞状态 → 返回 ReEvaluateGoal 动作 |
| ✅ `ReactionStats` 结构体 | P1 | AtomicU64 hits/misses + total() + hit_rate() |
| ✅ `ReactionLayerBuilder` | P0 | Builder 模式：`ReactionLayer::builder().add_rule(r1).add_rule(r2).build()` |
| ✅ 单元测试：每个内置规则 match/miss | P1 | 4 规则 × 3-4 场景 = 17 tests |
| ✅ 单元测试：Hit 时短路（不调用后续规则） | P0 | 22 tests, 0 failed |
| ✅ 单元测试：stats 计数正确 | P1 | |
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
| ✅ `MetaScoreWeights` 定义 | P0 | `verifier: 0.5, pred_error: 0.3, cost_remaining: 0.2`（可配置） |
| ✅ `CalibrationModel` trait 定义 | P0 | `async fn calibrate(state, decision_text) -> CalibrationResult`（用 &str 解耦） |
| ✅ `CalibrationResult` 结构体 | P0 | `raw_confidence, calibrated_confidence, should_retry, reasoning` |
| ✅ `MetaAction` 枚举 | P0 | Proceed / RetryDecision / RequestClarification / SwitchStrategy / DelegateToHuman / AbortOnBudget |
| ✅ `evaluate()` 三信号融合 | P0 | `meta_score = w1×verifier + w2×(1-pred_error) + w3×cost_remaining` + InteractionPattern 消费 |
| ✅ `MetacognitiveAssessment` 结构体 | P0 | `calibration, meta_score, knows_unknown, concept_drifting, budget_exhausted, suggested_action` |
| ✅ `compute_cost_remaining()` | P0 | 委托给 BudgetConsumed::cost_remaining_fraction() |
| ✅ `TTSSignal` 枚举 + `tts_signal()` | P0 | Normal/Degraded/Urgent/Abort 四级，classify_tts() 分档 |
| ✅ `calibrate_with_outcome()` | P0 | state.update_pred_error(actual) + 追加 CalibrationRecord + anomaly_detector.update() |
| ✅ `AnomalyDetector` 结构体 | P1 | 滑动窗口（50 条）+ EMA 基线更新 + drift_threshold=0.2 |
| ✅ `CalibrationHistory` 管理 | P1 | VecDeque 环形缓冲，容量 1000，push()/recent(n)/recent_avg_meta_score() |
| ✅ 单元测试：三信号融合公式计算正确 | P0 | 16 tests, 0 failed |
| ✅ 单元测试：TTS 分档边界（0.5/0.2/0.05） | P0 | |
| ✅ 单元测试：loop_detected → SwitchStrategy | P0 | |
| ✅ 单元测试：cost < 0.05 → AbortOnBudget | P0 | |
| ✅ 单元测试：anomaly detector 漂移检测 | P1 | |
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

### 3.4 agent-persona ✅（已完成）

> **测试结果：** 5 passed, 0 failed, 0 warnings

```
crates/agent-persona/
├── Cargo.toml
├── README.md               // 完整使用文档 + 关系图/履历/快照示例
└── src/
    ├── lib.rs              // Persona + PersonaSnapshot + PersonaContext ✅
    ├── identity.rs         // Identity（名称/角色/组织/背景/专长）✅
    ├── relationships.rs    // RelationshipGraph + Relationship + RelationType ✅
    └── history.rs          // PersonaHistory + PersonaEvent ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `Identity` 结构体 | P0 | `name, role, organization, background, expertise` + builder 方法 |
| ✅ `RelationshipGraph` 结构体 | P0 | HashMap: AgentId → `Relationship { trust, type, collaboration_count }` + `trusted_peers()` |
| ✅ `PersonaHistory` 结构体 | P1 | `Vec<PersonaEvent>` + `recent(n)` + `by_type()` 筛选 |
| ✅ `Persona` 结构体 | P0 | `version, identity, relationships, history` |
| ✅ `to_context_injection()` 实现 | P0 | 生成 `PersonaContext { name, role, expertise, trust_peers }` |
| ✅ `update_relationship()` 实现 | P0 | version += 1 + `adjust_trust()` |
| ✅ `snapshot()` 实现 | P0 | 生成 `PersonaSnapshot { version, identity, relationship_count }` |
| ✅ MVCC：版本号管理 | P1 | 主进程写入 version += 1，Sidecar 读快照 |
| ✅ 单元测试：关系更新 + 版本号变更 | P0 | 5 tests, 0 failed |
| ✅ 单元测试：snapshot + trusted_peers | P1 | |

**依赖：** `agent-types-core`（AgentId）

**验收标准（已验证）：**
```bash
cargo test -p agent-persona   # 5 passed, 0 failed, 0 warnings
cargo check -p agent-persona  # 0 errors, 0 warnings
```

---

### 3.5 agent-character ✅（已完成）

> **测试结果：** 6 passed, 0 failed, 0 warnings

```
crates/agent-character/
├── Cargo.toml
├── README.md               // 完整使用文档 + 价值观检查/偏好调整示例
└── src/
    ├── lib.rs              // Character + CharacterContext ✅
    ├── values.rs           // CoreValue + ValueEnforcement + ValueViolation + 3 个预设 ✅
    └── preferences.rs      // Preferences + UncertaintyStrategy + OutputStyle ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `CoreValue` 结构体 | P0 | `name, description, enforcement, forbidden_keywords` + `violates(action)` |
| ✅ `Preferences` 结构体 | P0 | `tool_preference, risk_tolerance, uncertainty_strategy, output_style` + builder |
| ✅ `Character` 结构体 | P0 | `core_values, preferences` |
| ✅ `check_core_values()` 实现 | P0 | 遍历 core_values，HardConstraint 违反 → `Err(ValueViolation)` |
| ✅ `to_context_injection()` 实现 | P0 | 生成 `CharacterContext { output_style, uncertainty_strategy, risk_tolerance }` |
| ✅ 内置 CoreValue 预设 | P1 | `privacy_first()`, `honesty_first()`, `no_destructive_actions()` |
| ✅ `UncertaintyStrategy` 枚举 | P0 | SearchFirst / AskUserFirst / BestGuessAndConfirm |
| ✅ `OutputStyle` 枚举 | P0 | Concise / Detailed / StepByStep |
| ✅ 单元测试：HardConstraint 违反检测 | P0 | 6 tests, 0 failed |
| ✅ 单元测试：SoftGuideline 不阻断 | P1 | |

**依赖：** `agent-types-core`（Action）

**关键约束：** Character.core_values 不可变（构造后不提供 setter）。✅

**验收标准（已验证）：**
```bash
cargo test -p agent-character   # 6 passed, 0 failed, 0 warnings
cargo check -p agent-character  # 0 errors, 0 warnings
```

---

## 4. 阶段 2：agent-mesh Agent 语义包装 ✅（已完成）

> **测试结果：** 11 passed, 0 failed, 0 warnings |
> **关联：** 需 uuid 依赖

```
crates/agent-mesh/
├── Cargo.toml
├── README.md               // 完整使用文档 + 9 种事件类型表
└── src/
    ├── lib.rs              // AgentMesh 门面 + re-exports ✅
    ├── topics.rs           // 4 通配符 + 8 精确 topic 常量 ✅
    ├── events/
    │   ├── mod.rs
    │   ├── state.rs        // StateSnapshotEvent ✅
    │   ├── task.rs         // TaskCreated / TaskCompleted / SubtaskDelegated / DelegationResult ✅
    │   ├── decision.rs     // DecisionMade / DecisionRetried ✅
    │   └── persona.rs      // PersonaUpdated / RelationshipChanged ✅
    └── registry.rs         // AgentTypeRegistry::register_all() ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ 定义 topic 命名空间常量 | P0 | 4 通配符（TOPIC_STATE/TASK/DECISION/PERSONA）+ 8 精确 topic |
| ✅ `StateSnapshotEvent` 封装 | P0 | event_id, agent_id, snapshot_json, snapshot_version, timestamp |
| ✅ `TaskCreated` / `TaskCompleted` 事件 | P0 | 封装 Task 生命周期 |
| ✅ `SubtaskDelegated` / `DelegationResult` 事件 | P1 | 封装协作委派 |
| ✅ `DecisionMade` / `DecisionRetried` 事件 | P0 | 封装元认知决策（meta_score, meta_action, tokens_used） |
| ✅ `PersonaUpdated` / `RelationshipChanged` 事件 | P1 | 封装 Persona 变更 |
| ✅ `AgentTypeRegistry` 初始化 | P0 | 启动期一次性注册全部 9 种事件类型（domain 用下划线） |
| ✅ `AgentMesh` 门面 | P0 | 包装 `EventMesh` + `FlowHandle`，构造时自动注册所有类型 |
| ✅ 单元测试：每种事件序列化/反序列化往返 | P0 | 11 tests, 0 failed |
| ✅ 单元测试：TypeRegistry 注册全部类型 | P0 | |

**关键设计：** 本 crate 是对 `uwu_event_mesh` 的薄包装，不重复实现任何底层机制。只定义 Agent 领域的 topic 命名空间和事件类型。

**验收标准（已验证）：**
```bash
cargo test -p agent-mesh   # 11 passed, 0 failed, 0 warnings
cargo check -p agent-mesh  # 0 errors, 0 warnings
```

---

## 5. 阶段 3：能力域 + FlowGraph + FlowEngine

> **依赖：** 阶段 0b（uwu_visual_script）、阶段 1（agent-state）、阶段 2（agent-mesh）
> **目标：** Perception/Memory/Reasoning/Execution 作为 visual_script NodeDefinition，FlowGraph 作为领域包装，FlowEngine 作为主循环执行器

### 5.1 agent-perception ✅（已完成）

> **测试结果：** 13 passed, 0 failed, 0 warnings |
> **关联：** 需 regex 依赖；移除了未使用的 agent-types-ext、agent-mesh 依赖

```
crates/agent-perception/
├── Cargo.toml
├── README.md               // 完整使用文档 + PII 策略/模式表
└── src/
    ├── lib.rs              // Perceiver trait + PerceptionPipeline + tests ✅
    ├── context.rs          // ContextDescriptor re-export + ParsedInput ✅
    └── pii.rs              // PiiScanner (5 种模式) + PiiStrategy (Mask/Encrypt/Remove) + tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `Perceiver` trait 定义 | P0 | `async fn perceive(raw_input: &str) -> ContextDescriptor` |
| ✅ `PerceptionPipeline` 结构体 | P0 | `run()` / `run_parsed()`，可组合 PiiScanner |
| ✅ 文本解析器（ParsedInput::from_text） | P0 | 集成在 context.rs |
| ✅ JSON 结构化解析器（ParsedInput::from_json） | P0 | 集成在 context.rs |
| ✅ PII 检测集成 | P1 | regex 5 种内置模式 + Mask/Encrypt/Remove 三策略 |
| ✅ 注册为 visual_script NodeDefinition | P0 | `"perception.observe"`: Impure + Async，feature = "visual-script" |
| ✅ 单元测试：文本解析 + PII 遮蔽 | P0 | 13 tests, 0 failed |
| ✅ 单元测试：作为 visual_script 节点执行 | P0 | 节点注册校验 + runner 异步验证（feature = "visual-script"），15 tests total |

**验收标准（已验证）：**
```bash
cargo test -p agent-perception   # 15 passed, 0 failed, 0 warnings
cargo check -p agent-perception  # 0 errors, 0 warnings
```

---

### W. agent-wiki ✅（已完成）

> **测试结果：** 12 passed, 0 failed, 0 warnings |
> **定位：** 多 Agent 协作的结构化知识库，MVCC 版本化 + 可插拔存储后端

```
crates/agent-wiki/
├── Cargo.toml
└── src/
    ├── lib.rs          // re-exports + 集成测试 ✅
    ├── page.rs         // WikiPage + WikiPageVersion + PageDiff + PageStatus ✅
    ├── repo.rs         // WikiRepo async trait（CRUD + search + filter + list）✅
    └── store.rs        // MemoryWikiStore（开发用）+ tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `WikiPage` 结构体 | P0 | page_id, title, content(md), tags, category, status, version_history, references |
| ✅ `WikiPageVersion` 版本历史 | P0 | version, title, content, edit_summary, edited_by, edited_at |
| ✅ MVCC 版本管理 | P0 | edit() → version += 1, rollback_to(), diff_versions() → PageDiff |
| ✅ `WikiRepo` trait | P0 | async CRUD + search + by_tag/by_category/by_status + list 分页 |
| ✅ `MemoryWikiStore` 实现 | P0 | HashMap 内存存储，标题去重，全文搜索 |
| ✅ 单元测试 | P0 | 12 tests, 0 failed |

**后续集成（全部完成）：**
- ✅ 接 `uwu_database` → DatabaseWikiStore (feature = "database", 14 tests)
- ✅ 接 `agent-crdt` → SharedState (ORSet/LWWRegister for wiki edits)
- ✅ 接 `agent-session` → Session.wiki + save_to_wiki / search_wiki
- ✅ 接 `agent-collaboration` → delegate_wiki_edit / delegate_wiki_create

**验收标准（已验证）：**
```bash
cargo test -p agent-wiki                         # 12 passed, 0 failed
cargo test -p agent-wiki --features database      # 14 passed, 0 failed
cargo check -p agent-wiki                        # 0 errors, 0 warnings
```
```

---

### 5.2 agent-memory ✅（已完成）

> **测试结果：** 10 passed, 0 failed, 0 warnings |
> **关联：** 移除了未使用的 agent-types-ext/agent-persona/agent-mesh/agent-crdt/uwu_database 依赖

```
crates/agent-memory/
├── Cargo.toml
├── README.md               // 完整使用文档 + 巩固策略表
└── src/
    ├── lib.rs              // MemoryFacade + RetrievedMemories + tests ✅
    ├── unified.rs          // UnifiedMemory（HashMap 实现 + 余弦检索）+ tests ✅
    ├── retrieve.rs         // RetrievalIntent ✅
    ├── consolidate.rs      // Episode + consolidate_episode() ✅
    ├── types.rs            // MemoryType + Memory + MemoryScore ✅
    └── embedding.rs        // Embedding + cosine_similarity + mock() + tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `MemoryType` 枚举 | P0 | Episodic / Semantic / Procedural / Working |
| ✅ `Memory` 结构体 | P0 | id, memory_type, content, embedding, score, state_snapshot, agent_id, access tracking |
| ✅ `MemoryScore` 结构体 | P0 | similarity + recency + frequency → total（三等权） |
| ✅ `UnifiedMemory` 结构体 | P0 | HashMap 内存实现，retrieve() / retrieve_typed() / persist / consolidate |
| ✅ `retrieve(intent)` — 默认检索 | P0 | 余弦相似度排序 + 阈值过滤 + 自动记录访问 |
| ✅ `retrieve_typed(intent, types)` — 按类型检索 | P1 | 仅检索指定 MemoryType |
| ✅ `persist_state(snapshot)` | P0 | State 快照 → Working 记忆 |
| ✅ `persist_persona(snapshot)` | P1 | Persona 快照 → Semantic 记忆 |
| ✅ `consolidate(episode)` | P0 | Episode → Episodic + Semantic + Procedural 记忆 |
| ✅ `Embedding::mock()` | P0 | 确定性伪嵌入，开发调试用 |
| ✅ `MemoryFacade` 门面 | P0 | 封装常用操作：retrieve() / persist_state() / consolidate() |
| ✅ 注册为 visual_script NodeDefinition | P0 | `"memory.retrieve"`: Impure + Async，feature = "visual-script" |
| ✅ 单元测试：retrieve 返回相关记忆 | P0 | 10 tests, 0 failed |
| ✅ 单元测试：persist_state → retrieve 往返 | P0 | |

**后续集成：**
- 接 `uwu_database::VectorStore` → 生产级向量检索（Qdrant/Pgvector/LanceDB）
- 接外部 embedding 服务 → OpenAI/本地模型替代 mock
- 接 `agent-mesh` → 记忆变更事件通知

**验收标准（已验证）：**
```bash
cargo test -p agent-memory   # 10 passed, 0 failed, 0 warnings
cargo check -p agent-memory  # 0 errors, 0 warnings
```

### 5.3 agent-reasoning ✅（已完成）

> **测试结果：** 12 passed, 0 failed, 0 warnings |
> **关联：** 移除了未使用的 agent-types-ext/agent-mesh/agent-uncertainty 依赖

```
crates/agent-reasoning/
├── Cargo.toml
├── README.md               // 完整使用文档 + TTS→策略映射表
└── src/
    ├── lib.rs              // ReasoningInput/Output + tests ✅
    ├── reasoner.rs         // Decision + Reasoner trait ✅
    ├── tot.rs              // ToTExplorer + ToTConfig + tests ✅
    ├── sandbox.rs          // SandboxEvaluator + tests ✅
    └── strategies.rs       // ReasoningStrategy + tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `Reasoner` trait 定义 | P0 | `async fn reason(state, goal, context) -> Decision` |
| ✅ `Decision` 结构体 | P0 | `actions: Vec<Action>, scores: Vec<f32>, reasoning: String` + best_action/best_score |
| ✅ `ToTExplorer` 结构体 | P1 | Beam Search：生成候选 → fork 推演 → 评分 → 剪枝（beam_width=3, max_depth=4） |
| ✅ fork 沙盒推演 | P0 | `SandboxEvaluator::evaluate_candidates()`: fork → apply_hypothetical → evaluate |
| ✅ 推理策略切换 | P0 | `ReasoningStrategy::from_cost_remaining()`: Normal/Degraded/Urgent/Abort + allows_tot/new_tools/should_abort |
| ✅ 注册为 visual_script NodeDefinition | P0 | `"reasoning.decide"`: Impure + Async，feature = "visual-script" |
| ✅ 单元测试：单步推理 | P0 | 12 tests, 0 failed |
| ✅ 单元测试：ToT beam search | P1 | |
| ✅ 单元测试：TTS 信号 → 策略切换 | P0 | |

**验收标准（已验证）：**
```bash
cargo test -p agent-reasoning   # 12 passed, 0 failed, 0 warnings
cargo check -p agent-reasoning  # 0 errors, 0 warnings
```

### 5.4 agent-execution ✅（已完成）

> **测试结果：** 20 passed, 0 failed, 0 warnings |
> **关联：** 移除了未使用的 agent-types-ext/agent-mesh/agent-tools/uwu_wasm 依赖；需 tokio time feature；新增 wasm-sandbox feature flag

```
crates/agent-execution/
├── Cargo.toml
├── README.md               // 完整使用文档 + 三种输出格式
└── src/
    ├── lib.rs              // ExecutionResult + Executor trait + ActionExecutor + tests ✅
    ├── mcp.rs              // McpClient + McpResult + tests ✅
    ├── output.rs           // OutputFormatter + OutputFormat + tests ✅
    └── wasm.rs             // WasmExecutor + tests (feature = "wasm-sandbox") ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `Executor` trait 定义 | P0 | `async fn execute(action, state) -> ExecutionResult` |
| ✅ `ActionExecutor` 结构体 | P0 | `execute_action()` + `execute_batch()` + with_mcp/with_max_parallel |
| ✅ MCP 工具调用 | P0 | `McpClient`: mock 模式 + HTTP 模式（feature = "http"，POST 真实 MCP Server），优雅降级 |
| ✅ OutputFormatter | P0 | PlainText / Json / Markdown 三种输出格式 |
| ✅ WASM 沙箱执行 | P2 | `WasmExecutor`: uwu_wasm::Sandbox 集成 + Policy + 模块注册 + add/sub/mul 已验证（feature = "wasm-sandbox"） |
| ✅ 注册为 visual_script NodeDefinition | P0 | `"execution.act"`: Impure + Async，feature = "visual-script" |
| ✅ 单元测试：MCP 工具调用 mock | P0 | 10 tests, 0 failed（含 mock/http 双模式） |
| ✅ 单元测试：WASM 沙箱执行 | P2 | 11 tests (wasm module register/execute/batch/policy/missing params/unknown module)，0 failed |

**验收标准（已验证）：**
```bash
cargo test -p agent-execution                        # 9 passed, 0 failed, 0 warnings
cargo test -p agent-execution --features wasm-sandbox  # 20 passed, 0 failed, 0 warnings
cargo check -p agent-execution                       # 0 errors, 0 warnings
```

### 5.5 FlowGraph + FlowEngine ✅（已完成）

> **测试结果：** 13 passed, 0 failed, 0 warnings |
> **关联：** 简化了 agent-core 依赖（仅 agent-state + agent-types-core）；FlowGraph 为纯配置层

```
crates/agent-core/
├── Cargo.toml
├── README.md               // 完整使用文档 + 管道拓扑图
└── src/
    ├── lib.rs              // re-exports + 集成测试 ✅
    ├── flow.rs             // FlowGraph + FlowConfig + Stage + FlowEdge + tests ✅
    ├── engine.rs           // FlowEngine + FlowContext + Decision + tests ✅
    └── capability.rs       // CapabilityRegistry + CapabilityHandler + tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `FlowGraph` 结构体 | P0 | 声明式管道配置，standard() / high_security() / custom() |
| ✅ `FlowGraph::standard()` | P0 | P→M→R→E 标准管道 |
| ✅ `FlowGraph::high_security()` | P1 | P→M→R→V→R→E（含 Validate 验证回边） |
| ✅ `FlowConfig::custom()` | P1 | 自定义 stages + edges |
| ✅ `add_edge_dynamic()` | P0 | 运行时动态添加阶段和边 |
| ✅ `CapabilityRegistry` 结构体 | P0 | HashMap<Stage, Vec<Box<dyn CapabilityHandler>>>，同阶段多处理器 |
| ✅ `FlowEngine` 结构体 | P0 | `run(flow, input, state) -> FlowContext`，按拓扑执行各阶段 |
| ✅ FlowEngine 与 visual_script VM 集成 | P0 | `"flow.run"` 节点: Impure + Async，feature = "visual-script" |
| ✅ 单元测试：standard/high_security 管道执行 | P0 | 13 tests, 0 failed |
| ✅ 单元测试：add_edge_dynamic() 更新 | P0 | |
| ✅ 单元测试：FlowEngine 完整执行 P→M→R→E | P0 | |

**验收标准（已验证）：**
```bash
cargo test -p agent-core   # 13 passed, 0 failed, 0 warnings
cargo check -p agent-core  # 0 errors, 0 warnings
```

---

## 6. 阶段 4：Session 主循环编排 ✅（已完成）

> **测试结果：** 9 passed, 0 failed, 0 warnings |
> **关联：** 移除了 agent-types-ext/agent-mesh 依赖；Metacognition 改为非 Arc（Session 独占）

```
crates/agent-session/
├── Cargo.toml
├── README.md               // 完整使用文档 + 决策流程图
└── src/
    ├── lib.rs              // Session + process_turn() + tests ✅
    ├── turn.rs             // ConversationTurn ✅
    ├── intent.rs           // IntentTracker + tests ✅
    ├── history.rs          // ConversationHistory + tests ✅
    └── snapshot.rs         // SessionSnapshot ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `Session` 结构体 | P0 | 持有五维 + P→M→R→E 管道（PerceptionPipeline + MemoryFacade + Reasoner + ActionExecutor） + ConversationHistory + IntentTracker |
| ✅ `process_turn()` 完整主循环 | P0 | 6 段式：Reaction → FlowGraph(真实P→M→R→E) → Metacognition → MetaAction 6 分支 → Execution + Memory持久化 → Calibrate |
| ✅ `enrich_input()` 实现 | P1 | PersonaContext + CharacterContext 注入 |
| ✅ `execute_reaction()` 实现 | P0 | Hit → 0 token 直接执行 |
| ✅ MetaAction 全部分支处理 | P0 | Proceed / RetryDecision(rollback+重推理) / RequestClarification / SwitchStrategy(降级) / DelegateToHuman / AbortOnBudget |
| ✅ `execute_and_update()` 实现 | P0 | apply_action → 修正 State |
| ✅ `snapshot()` 实现 | P0 | SessionSnapshot { state_snapshot, persona_version, turn_count, total_tokens } |
| ✅ `IntentTracker` 实现 | P1 | update + infer + is_stuck 循环检测 |
| ✅ `checkpoint` 管理 | P1 | `auto_checkpoint()` → 在 MetaAction 分支前自动打 checkpoint |
| ✅ 单元测试：process_turn + checkpoint + 集成 | P0 | 11 tests, 0 failed |


**验收标准（已验证）：**
```bash
cargo test -p agent-session   # 11 passed, 0 failed, 0 warnings
cargo check -p agent-session  # 0 errors, 0 warnings
```

---

## 7. 阶段 5：Task + Collaboration

> **依赖：** 阶段 4（Session）
> **目标：** 持久任务管理 + 多 Agent 协作委派

### 7.1 agent-task ✅（已完成）

> **测试结果：** 2 passed, 0 failed |
> **关联：** 移除了 agent-types-ext/agent-state/agent-mesh/tokio 依赖

```
crates/agent-task/
├── Cargo.toml
├── README.md               // 完整使用文档 + DAG 调度示例
└── src/
    ├── lib.rs              // Task + Goal + TaskStatus ✅
    ├── subtask.rs          // Subtask + SubtaskDag + SubtaskStatus + tests ✅
    ├── manifest.rs         // TaskManifest + AgentCard ✅
    ├── delegation.rs       // DelegationPolicy + DiscoveryStrategy + FallbackStrategy ✅
    ├── settlement.rs       // SettlementPolicy + SettlementMode ✅
    └── scheduler.rs        // SubtaskScheduler ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `Task` 结构体 | P0 | task_id, goal, status, subtask_dag, max_retries, manifest |
| ✅ `SubtaskDag` 结构体 | P0 | nodes + edges + ready_nodes() |
| ✅ `Subtask` 结构体 | P0 | id, index, description, status, assigned_agent, max_retries, timeout |
| ✅ `AgentCard` 结构体 | P0 | agent_id, name, capabilities, role, trust_score, endpoint |
| ✅ `DelegationPolicy` 结构体 | P0 | ExactCapability / LoadBalanced / TrustRanked / Auction |
| ✅ `SettlementPolicy` 结构体 | P0 | Free / FixedPrice / Metered / Auction |
| ✅ `check_ready()` 实现 | P0 | DAG 拓扑检查可执行 subtask |
| ✅ `SubtaskScheduler` 实现 | P0 | next_ready() + is_complete() + progress() |
| ✅ `update_progress()` 实现 | P0 | 根据 DAG 进度更新 TaskStatus（Running → Completed） |
| ✅ 单元测试：DAG + update_progress + SettlementPolicy | P0 | 8 tests, 0 failed |

**验收标准：**
```bash
cargo test -p agent-task   # 8 passed, 0 failed
cargo check -p agent-task  # 0 errors, 0 warnings
```

### 7.2 agent-collaboration ✅（已完成）

> **测试结果：** 5 passed, 0 failed |
> **关联：** 移除了 agent-types-ext/agent-state/agent-persona/agent-mesh/agent-crdt/dashmap 依赖

```
crates/agent-collaboration/
├── Cargo.toml
├── README.md               // 完整使用文档 + 委派流程图
└── src/
    ├── lib.rs              // Collaboration + delegate/on_delegation_complete + tests ✅
    ├── registry.rs         // AgentRegistry + AgentDescriptor + tests ✅
    ├── delegate.rs         // DelegationId + DelegationState + DelegationResult + tests ✅
    └── negotiate.rs        // NegotiationResult ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `AgentRegistry` 结构体 | P0 | HashMap<AgentId, AgentDescriptor> + find_by_capability + best_for_capability |
| ✅ `Collaboration` 结构体 | P0 | registry + pending_delegations |
| ✅ `delegate()` 实现 | P0 | 按 capability 选择最优 Agent → 创建 DelegationResult |
| ✅ `on_delegation_complete()` 实现 | P0 | 接收结果 → complete() → 更新 state |
| ✅ `negotiate()` 实现 | P1 | NegotiationResult: accepted / rejected / counter_offer |
| ✅ 单元测试：capability 匹配 | P0 | 5 tests, 0 failed |
| ✅ 单元测试：委派 → 完成 | P0 | |

**验收标准：**
```bash
cargo test -p agent-collaboration   # 5 passed, 0 failed
cargo check -p agent-collaboration  # 0 errors, 0 warnings
```

---

## 8. 阶段 6：LearnNode 自学习 ✅（已完成）

> **测试结果：** 7 passed, 0 failed |
> **关联：** 移除了 agent-mesh/agent-guard 依赖

```
crates/agent-learning/
├── Cargo.toml
├── README.md               // 完整使用文档 + 5 层防护说明
└── src/
    ├── lib.rs              // Episode + EpisodeOutcome ✅
    ├── trigger.rs          // LearnCondition trait + LearnDecision + LearnTrigger + tests ✅
    ├── skill.rs            // SkillTarget + SkillVersion + tests ✅
    └── conditions/
        └── mod.rs          // 3 个条件实现 + tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `LearnCondition` trait 定义 | P0 | `async fn should_learn(episode, state) -> LearnDecision` |
| ✅ `LearnDecision` 枚举 | P0 | Skip / ConsolidateEpisode / ExtractSkill / UpdatePreference |
| ✅ `LearnTrigger` 结构体 | P0 | 顺序评估条件，首个命中即返回 |
| ✅ `SkillTarget` 枚举 | P0 | LocalCode / McpRemote / LocalPreference |
| ✅ `SkillVersion` 结构体 | P0 | version_id, hash, verified, active + verify()/deactivate() |
| ✅ `SignificantErrorCondition` | P0 | pred_error > 阈值 → ExtractSkill |
| ✅ `NewPatternCondition` | P1 | Success + confidence ≥ 阈值 → ExtractSkill |
| ✅ `UserConfirmedCondition` | P1 | Success → ConsolidateEpisode |

**验收标准：**
```bash
cargo test -p agent-learning   # 18 passed, 0 failed
cargo check -p agent-learning  # 0 errors, 0 warnings
```
| ✅ Guard egress 集成 | P0 | SkillGate::check_egress() → GuardLayer egress rules |
| ✅ 沙箱验证新 Skill | P0 | SkillGate::verify_in_sandbox(): fork() → apply → evaluate |
| ✅ 回滚机制 | P1 | SkillRegistry::rollback(): deactivate current → activate previous |
| ✅ 单元测试：SignificantError 触发学习 | P0 | conditions::tests (4 tests) |
| ✅ 单元测试：McpRemote 需要 Guard egress 通过 | P0 | guard::tests (4 egress tests) |
| ✅ 单元测试：SkillVersion 沙箱验证 | P0 | guard::tests (2 sandbox tests) |
| ✅ 单元测试：回滚至上一版本 | P1 | guard::tests (4 registry tests) |

---

## 9. 阶段 7：GuardLayer 安全守卫 ✅（已完成）

> **测试结果：** 21 passed, 0 failed |
> **关联：** 移除了 agent-types-ext 依赖；修复了 enforce() 中 ParameterRule 使用 action.params；补全 12→21 tests

```
crates/agent-guard/
├── Cargo.toml
├── README.md               // 完整使用文档 + 五层闸门表 + 8 个内置规则表
└── src/
    ├── lib.rs              // 5 trait + GuardLayer + GuardBuilder + enforce + tests ✅
    ├── audit.rs            // AuditLog + AuditEvent + test ✅
    └── rules/
        └── mod.rs          // 8 个内置规则 + tests ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ `InstructionRule` trait | P0 | `async fn check(action) -> Option<GuardViolation>` |
| ✅ `ParameterRule` trait | P0 | `async fn check(action, params) -> Option<GuardViolation>` |
| ✅ `CapabilityRule` trait | P0 | `async fn check(action) -> Option<GuardViolation>` |
| ✅ `BudgetRule` trait | P0 | `async fn check(tokens, max_tokens, retries, max_retries) -> Option<GuardViolation>` |
| ✅ `EgressRule` trait | P0 | `async fn check_egress(target) -> Option<GuardViolation>` |
| ✅ `GuardLayer` + `enforce()` | P0 | 顺序五层检查 → allowed actions 或 blocked violations |
| ✅ `GuardBuilder` | P0 | Builder 模式：编译期注册 |
| ✅ `NoRmRfRule` | P0 | 禁止 rm_rf / delete_all / drop_table / format |
| ✅ `NoShellExecutionRule` | P0 | 禁止 exec / system / shell |
| ✅ `FileSizeLimitRule` | P1 | 文件大小上限 |
| ✅ `PortAllowlistRule` | P1 | 端口白名单 |
| ✅ `TokenBudgetRule` | P0 | Token 耗尽检查 |
| ✅ `RetryBudgetRule` | P0 | 重试次数检查 |
| ✅ `McpWriteAllowlistRule` | P0 | MCP 写入白名单 |
| ✅ `NoNetworkToInternalRule` | P1 | 禁止 10.x / 192.168.x / 172.16.x |
| ✅ `AuditLog` | P0 | total_events / blocked_count / recent(n) + 4 tests |

**验收标准：**
```bash
cargo test -p agent-guard   # 21 passed, 0 failed
cargo check -p agent-guard  # 0 errors, 0 warnings
```
| ✅ 单元测试：rm -rf 被阻断 | P0 | NoRmRfRule + enforce |
| ✅ 单元测试：Token 耗尽 → Warning | P0 | TokenBudgetRule + enforce |
| ✅ 单元测试：未授权 MCP 写入被阻断 | P0 | McpWriteAllowlistRule + check_egress |
| ✅ 单元测试：所有规则通过 → 放行 | P0 | enforce_allows_safe_actions |
| ✅ 单元测试：enforce() 部分通过 → 返回 allowed + blocked | P0 | enforce_partial_pass_some_allowed_some_blocked |

**关键约束：** GuardLayer 构造后不可修改规则集。Agent 无法在运行时绕过或提升自己的权限。

---

## 10. 阶段 8：Sidecar 独立进程

> **依赖：** 阶段 2（agent-mesh）、阶段 6（agent-learning）、阶段 7（GuardLayer）
> **目标：** Consolidator + Monitor 作为独立进程运行

### 10.1 agent-sidecar-consolidator ✅（已完成）

> **状态：** 可编译运行 |
> **关联：** 移除了 agent-mesh/uwu_event_mesh 依赖；使用 mock Episode 主循环

```
crates/agent-sidecar-consolidator/
├── Cargo.toml
├── README.md               // 进程文档 + 流程图
└── src/
    ├── lib.rs              // Consolidator 库（可嵌入）+ channel-based 循环 ✅
    └── main.rs             // 独立二进制入口 ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ LearnTrigger 评估 | P0 | 3 个条件链：SignificantError → NewPattern → UserConfirmed |
| ✅ Guard egress 博弈 | P0 | McpRemote → check_egress() 通过才写入 |
| ✅ Guard enforce | P0 | ExtractSkill 前 enforce 检查 |
| ✅ UnifiedMemory 持久化 | P0 | consolidate(episode) |
| ✅ Channel-based 长期运行 | P0 | tokio::sync::mpsc 消费 Episode 流，无固定迭代限制 |
| ✅ NATS/JetStream 连接 | P0 | uwu_nats_bridge crate |
| ⬜ 集成测试端到端 | P0 | 延后（需完整 agent-mesh 通道） |

### 10.2 agent-sidecar-monitor ✅（已完成）

> **状态：** 可编译运行，3 tests |
> **关联：** AnomalyDetector + MetacognitiveReport 已提取为公共库

```
crates/agent-sidecar-monitor/
├── Cargo.toml
├── README.md               // 进程文档 + 异常检测说明
└── src/
    ├── lib.rs              // AnomalyDetector + MetacognitiveReport + run_monitor()（可嵌入）✅
    └── main.rs             // 独立二进制入口 ✅
```

| 任务 | 优先级 | 说明 |
|---|---|---|
| ✅ 滑动窗口异常检测 | P0 | window=50, drift_threshold=0.2, EMA baseline |
| ✅ `MetacognitiveReport` 生成 | P0 | tokio::select 事件驱动 + 定期报告 + report channel 输出 |
| ✅ 告警输出 | P1 | drift_detected → 日志摘要 |
| ✅ Channel-based 长期运行 | P0 | tokio::sync::mpsc 消费 pred_error 流，优雅关闭 |
| ✅ 单元测试 | P1 | 3 tests（defaults/feed_mean/detect_drift） |
| ✅ NATS/JetStream 连接 | P0 | uwu_nats_bridge crate，monitoring 通道走 JetStream 持久化 |
| ⬜ 集成测试 | P0 | 延后 |

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
                    集成里程碑                              │
                    ├── mesh/core/task+collab → session       │
                    ├── uwu_database → agent-memory           │
                    ├── crdt → agent-collaboration            │
                    ├── uncertainty → agent-metacognition     │
                    └── wiki → session+collab+database        │
                                                              │
                    阶段 9                                   │
                    └── 集成测试 + 性能基准 ← 全部             │
```

> **注:** agent-state-short/mid/long 已删除（合并回 agent-state）。全仓现为 30 个 crate。

---

> **下一优先事项：** 启动阶段 1 — `agent-state` crate。
> State 是整个架构的根基，所有其他模块都依赖它。建议从 `AgentState` + `fork()` + `snapshot()` + `evaluate()` 的核心路径开始，快速出一个可编译可测试的 MVP。
