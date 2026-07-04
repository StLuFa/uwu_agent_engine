# 把 uwu context db 接入 uwu agent engine 指南

> 仓库：https://github.com/StLuFa/uwu_agent_engine.git
> 结论先行：**仓库里已经内置了完整的接入层 `crates/agent-context-db`**，你不需要从零写桥接代码。要做的是在 `agent-session` 的装配点（composition root）把存储实现 + 五维 bridge 注入到 `Session`。本文给出最小可行接入路径。

---

## 1. 先理解架构：context-db 与 engine 的边界在哪

uwu_agent_engine 采用**端口/适配器**架构。`uwu_context_db` 不是外挂模块，而是引擎五维（Reaction / State / Metacognition / Persona / Character）的**持久化与冷归档后端**。两者通过 `agent-context-db` crate 的 bridge 耦合。

### 1.1 数据流定位

```
热路径（内存，派生层真值源）          冷路径（context-db，事实层）
┌─────────────────────────┐         ┌──────────────────────────────┐
│ AgentState (agent-state)│ ──checkpoint──▶ StateBridge            │
│ CalibrationHistory       │ ──evict──────▶ MetacogBridge          │
│ Character.core_values    │ ──gates──────▶ CharacterConstraint    │
│ ReactionLayer            │ ◀──induce───── ReactionLearner        │
│ AgentMesh                │ ◀──publish──── EventMeshBridge        │
└─────────────────────────┘         └──────────────────────────────┘
        ▲                                     │
        │  process_turn 主循环                 │ 窄端口 trait
        │                                     ▼
┌─────────────────────────┐         ┌──────────────────────────────┐
│ Session (agent-session) │◀────────│ FsOps / ContentRepo /        │
│  持有五维 + P→M→R→E      │  注入    │ VersionStore / LlmClient     │
└─────────────────────────┘         └──────────────────────────────┘
                                              ▲
                                  注入实现 │
                    ┌───────────────────────┴───────────────────────┐
                    │ MemoryContextStore (testkit, 测试/开发)        │
                    │ PgContextStore + UwuVectorIndex (storage, 生产)│
                    └────────────────────────────────────────────────┘
```

### 1.2 关键 crate 一览

| crate | 角色 | 依赖 uwu? |
|-------|------|-----------|
| `context-db-core` | 通用核心：`uwu://` URI + 三层模型 + 窄端口 trait（`FsOps`/`ContentRepo`/`VersionOps`/`TenantOps`）+ `LlmClient`/`VectorIndex` | 否 |
| `context-db-retrieve` | 分层检索 + 意图分析 + 幻觉检测 | 否 |
| `context-db-version` | DAG 版本 + Branch/Tag + CRDT merge + 时间旅行 | 否 |
| `context-db-storage` | **生产装配根** `ContextDbService` + `PgContextStore` + `UwuVectorIndex` | 否 |
| `context-db-testkit` | **测试装配根** `MemoryContextStore` + `MemoryVersionStore` | 否 |
| **`agent-context-db`** | **五维桥接层**（接入引擎的唯一耦合点） | 是 |
| `context-db-session` | 两阶段 commit 会话压缩 | 否 |
| `context-db-parse` | MemoryExtractor / TrajectoryExtractor | 否 |
| `context-db-compressor` | tokio mpsc 异步语义处理队列 | 否 |

> 接入 engine 时，你**只直接依赖 `agent-context-db`**，其余通过它间接传递。

---

## 2. 五个 Bridge：context-db 与五维的对接点

全部位于 `crates/agent-context-db/src/`。每个 bridge 都通过注入的 core 窄端口访问存储，不 `use` 任何后端具体 struct。

| Bridge | 对接维度 | 核心方法 | 依赖端口 |
|--------|---------|---------|---------|
| `StateBridge` | State | `load` / `checkpoint` / `fork` / `promote_fork` / `discard_fork` / `compare_fork_pred_error` | `FsOps + ContentRepo` + `VersionStore` |
| `MetacogBridge` | Metacognition | `log_pred_error`（冷归档）/ `retrieve_calibration`（冷热合并检索） | `FsOps + ContentRepo` |
| `CharacterConstraint` | Character | `check_write` / `check_write_sync`（关键词 + LLM 双层） | `LlmClient`（可选） |
| `WriteGate` (sandbox) | Character + Guard | `gate` → `Pass / Reject / Quarantine` | `CharacterConstraint` + `SemanticSandbox` |
| `EventMeshBridge` | Mesh | `emit_written/retrieved/consolidated/...` → uwu_event_mesh | `MeshPublisher`（trait） |
| `ReactionLearner` | Reaction + Learning | `induce_rules`（从 experiences 归纳新规则） | `LlmClient` + `FsOps` |

### 2.1 真值源边界（重要，别踩坑）

参考 `ARCHITECTURE.md §6.3`，每个维度都有**派生层（内存热态）**与**事实层（context-db 冷存）**的边界：

| 维度 | 派生层（热，零 IO） | 事实层（冷，可重算） |
|------|---------------------|---------------------|
| State | `accumulated_pred_error: f32` | `uwu://.../state/{scope}/snapshot.json` |
| Metacog | `CalibrationHistory` 环形缓冲 | `uwu://.../metacog/pred_errors/{ts}.json` |
| Character | `core_values` 内存 | `uwu://.../character/core_values.md` |

**规则**：热路径只读写内存标量；evict / checkpoint / fork 时才落 context-db。`MetacogBridge::retrieve_calibration` 会自动合并冷热两源（同 ts 热覆盖冷）。

---

## 3. 标准接入流程（六步）

### Step 1：在 `agent-session` 的 `Cargo.toml` 添加依赖

```toml
[dependencies]
# 接入层（必须）
agent-context-db = { path = "../agent-context-db" }
# core 窄端口 trait（写 trait bound 时需要）
agent-context-db-core = { path = "../../pkg/uwu_context_db/context-db-core" }
# 版本层（StateBridge::fork 需要 VersionStore）
agent-context-db-version = { path = "../../pkg/uwu_context_db/context-db-version" }

[dev-dependencies]
# 测试时用内存后端
agent-context-db-testkit = { path = "../../pkg/uwu_context_db/context-db-testkit" }
```

生产环境再追加：

```toml
agent-context-db-storage = { path = "../../pkg/uwu_context_db/context-db-storage" }
```

### Step 2：选后端实现并构造 `ContextDbService`

**测试/开发**（`MemoryContextStore`，零外部依赖）：

```rust
use agent_context_db_testkit::{MemoryContextStore, MemoryVersionStore};
use std::sync::Arc;

let store = Arc::new(MemoryContextStore::new());
let versions = Arc::new(MemoryVersionStore::new());
```

**生产**（`PgContextStore`，需 PG + Qdrant）：

```rust
use agent_context_db_storage::{ContextDbService, PgContextStore, UwuVectorIndex};
use uwu_database::DbPool;

let pool = DbPool::connect("postgres://...").await?;
let store = Arc::new(PgContextStore::new(pool));
let index = Arc::new(UwuVectorIndex::new(qdrant_client));
let service = ContextDbService::new(store.clone(), index);
// service.fs_ops() / service.content_repo() 按需取窄端口
```

### Step 3：构造五维 Bridge

```rust
use agent_context_db_uwu::{
    StateBridge, MetacogBridge, CharacterConstraint,
    SemanticSandbox, WriteGate, EventMeshBridge,
    HttpLlmClient, CoreValue,
};

let llm = Arc::new(HttpLlmClient::new("https://api.openai.com/v1", "sk-..."));

// State 桥接（需要 VersionStore 做 fork/promote）
let state_bridge = Arc::new(StateBridge::new(store.clone(), versions.clone()));

// Metacog 桥接（只需内容端口）
let metacog_bridge = Arc::new(MetacogBridge::new(store.clone()));

// Character 写入约束 + 安全沙箱
let cc = CharacterConstraint::with_llm(core_values(), llm.clone());
let sandbox = SemanticSandbox::new(llm.clone());
let write_gate = Arc::new(WriteGate::new(cc, sandbox));

// EventMesh 桥接（注入 engine 的 FlowHandle 适配器）
let mesh_bridge = EventMeshBridge::new().with_mesh(flow_handle_adapter);
```

### Step 4：扩展 `Session` struct 持有 bridge

当前 `crates/agent-session/src/lib.rs` 的 `Session` 还没有 context-db 字段（它现在只用 `MemoryFacade` 内存记忆）。接入方式：加 `Option` 字段，保持向后兼容。

```rust
pub struct Session {
    // ... 现有五维 + P→M→R→E 字段保持不变 ...

    /// context-db 接入（可选，未注入时降级为纯内存模式）
    pub context_db: Option<ContextDbHandles>,
}

/// 聚合所有 context-db bridge 的句柄
pub struct ContextDbHandles {
    pub state_bridge: Arc<StateBridge<MemoryContextStore, MemoryVersionStore>>,
    pub metacog_bridge: Arc<MetacogBridge<MemoryContextStore>>,
    pub write_gate: Arc<WriteGate>,
    pub mesh_bridge: EventMeshBridge,
}
```

> 泛型参数可根据需要用 `Arc<dyn FsOps + ContentRepo>` + `Arc<dyn VersionStore>` 替代具体类型，避免泛型污染 `Session`。

### Step 5：在 `process_turn` 主循环的对应阶段调用 bridge

这是接入的**核心改动**。下面标出 `agent-session/src/lib.rs` 六段式主循环中每个 bridge 的挂载点：

```rust
pub async fn process_turn(&mut self, raw_input: &str) -> TurnResult {
    self.turn_count += 1;
    let input = self.enrich_input(raw_input);

    // 1. Reaction 拦截
    //    └─（可选）ReactionLearner::induce_rules 在 sidecar consolidator 中异步跑，
    //       产出的 NewRule 通过 EventMeshBridge 下发到 ReactionLayer
    if let Reaction::Hit(action) = self.reaction.intercept(&self.state).await {
        // ...
    }

    // 2. FlowGraph 管道 (P→M→R→E)
    //    └─ Memory 检索阶段可换用 context-db-retrieve 的 Retriever
    //       （IntentAnalyzer → VectorIndex → FsOps::grep/read → Rerank）
    let decision = self.run_flowgraph(&input).await;

    // 3. Metacognition 评估
    let assessment = self.metacognition.evaluate(&self.state, &decision.command).await;

    // 4. MetaAction 分支处理（fork 推演接入点）
    match assessment.suggested_action {
        MetaAction::SwitchStrategy => {
            if let Some(ref cdb) = self.context_db {
                // ★ StateBridge::fork 开推演沙盒
                let fork = cdb.state_bridge.fork(&agent_id, StateScope::Mid).await?;
                // ... 在 fork 分支上跑降级策略 ...
                let delta = cdb.state_bridge
                    .compare_fork_pred_error(&agent_id, StateScope::Mid, &fork).await?;
                if delta < 0.0 {
                    cdb.state_bridge.promote_fork(&fork, MergeStrategy::FastForward).await?;
                } else {
                    cdb.state_bridge.discard_fork(&fork).await?;
                }
            }
            // ...
        }
        _ => {}
    }

    // 5. 执行 + 收集结果（写入约束接入点）
    let result = self.execute_and_update(decision, raw_input).await;
    //    └─ execute_and_update 内部持久化前先过 WriteGate：
    if let Some(ref cdb) = self.context_db {
        let entry = build_context_entry(&result);
        match cdb.write_gate.gate(&entry).await? {
            SandboxVerdict::Pass => { /* 写入 */ }
            SandboxVerdict::Reject { reason, .. } => { /* 写 .quarantine */ }
            SandboxVerdict::Quarantine { .. } => { /* 人工审核 */ }
        }
    }

    // 6. Metacognition 在线校准 + 冷归档
    self.metacognition.calibrate_with_outcome(/* ... */);
    if let Some(ref cdb) = self.context_db {
        // ★ CalibrationHistory evict 时落冷存
        for sample in self.metacognition.drain_evicted_samples() {
            cdb.metacog_bridge.log_pred_error(&agent_id, &sample, tenant).await?;
        }
    }

    // 7. 发布事件 + State checkpoint
    if let Some(ref cdb) = self.context_db {
        let snap = StateSnapshot::new(/* ... */);
        cdb.state_bridge.checkpoint(&agent_id, StateScope::Mid, &snap, tenant).await?;
        cdb.mesh_bridge.emit_written(&uri, version, Some(&agent_id));
    }
    // ...
}
```

### Step 6：装配根（main / 二进制入口）串起来

```rust
// main.rs 或 agent-session 的 builder
let session = Session::builder()
    .with_five_dimensions(/* ... */)
    .with_context_db(ContextDbHandles {
        state_bridge,
        metacog_bridge,
        write_gate,
        mesh_bridge,
    })
    .build();
```

---

## 4. 最小验证：跑通 m3 集成测试

仓库已自带 `crates/agent-context-db/tests/m3_integration.rs`，它用 `MemoryContextStore + MemoryVersionStore` 验证 State fork/checkpoint + Metacog 归档 + Character 约束三件套。这是接入是否正确的**最小编译/运行验证**：

```bash
cd uwu_agent_engine
cargo test -p agent-context-db --test m3_integration
cargo test -p agent-context-db --test l4_l5_integration
```

通过后再在 `agent-session` 里加一个端到端测试：构造带 `context_db` 字段的 `Session`，跑 `process_turn`，断言 `state_bridge.load(...)` 能读回 checkpoint。

---

## 5. 按维度的接入细节

### 5.1 State 维度（`StateBridge`）

```rust
let bridge = StateBridge::new(store, versions);

// checkpoint：把 AgentState 序列化为 StateSnapshot 落盘
bridge.checkpoint("a1", StateScope::Mid, &snap, tenant).await?;

// fork 推演：返回 ForkHandle，后续写入自动进 fork 分支
let fork = bridge.fork("a1", StateScope::Mid).await?;

// 比较预测误差，决定晋升或回滚
let delta = bridge.compare_fork_pred_error("a1", StateScope::Mid, &fork).await?;
if delta < 0.0 {
    bridge.promote_fork(&fork, MergeStrategy::FastForward).await?;
} else {
    bridge.discard_fork(&fork).await?;
}

// load：优先 L2 Detail（PG blob），fallback L1 Overview，再 fallback L0 Abstract
let loaded = bridge.load("a1", StateScope::Mid).await?;
```

URI 规则：`uwu://default/agent/{id}/state/{short|mid|long}/snapshot.json`。

### 5.2 Metacognition 维度（`MetacogBridge`）

只在 `CalibrationHistory` 环形缓冲 evict 时调用，**不在每轮 turn 调用**（热路径零 IO）：

```rust
// evict 时冷归档
bridge.log_pred_error("a1", &sample, tenant).await?;

// 校准时冷热合并检索
let results = bridge.retrieve_calibration(
    "a1",
    TimeWindow { from_ts, to_ts },
    &hot_samples,  // 内存中未 evict 的记录
).await?;
```

URI 规则：`uwu://default/agent/{id}/metacog/pred_errors/{ts}.json`。

### 5.3 Character 维度（`CharacterConstraint` + `WriteGate`）

两层闸门，挂在 `ContentRepo::write` 之前：

1. **关键词快速路径**（同步，零 LLM）：`forbidden_terms` 子串匹配 L0 + L1。
2. **LLM 语义审查**（异步）：注入 `LlmClient` 后启用，按 `CoreValue.description` 做语义判定。

```rust
let cc = CharacterConstraint::with_llm(core_values, llm);
let gate = WriteGate::new(cc, SemanticSandbox::new(llm));
match gate.gate(&entry).await? {
    SandboxVerdict::Pass => store.write(entry).await?,
    SandboxVerdict::Reject { reason, rule } => /* 写 .quarantine */,
    SandboxVerdict::Quarantine { risk_score, .. } => /* 人工审核 */,
}
```

> LLM 不可用时默认放行，避免阻塞写入（安全侧不阻塞）。

### 5.4 Reaction + Learning 维度（`ReactionLearner`）

不在主循环同步调用，由 **sidecar consolidator** 进程异步触发：

```rust
let learner = ReactionLearner::new(llm, fs_ops);
let new_rules = learner.induce_rules(&experience_dir).await?;
// 产出的 NewRule 通过 EventMeshBridge 下发到主进程的 ReactionLayer
```

### 5.5 Mesh 维度（`EventMeshBridge`）

`EventMeshBridge` 不直接依赖 `uwu_event_mesh`，而是通过 `MeshPublisher` trait 注入。在装配根用 `FlowHandle` 适配：

```rust
pub trait MeshPublisher: Send + Sync {
    fn publish(&self, topic: &str, payload: &[u8]);
}

// engine 装配时实现 MeshPublisher for FlowHandle，注入 bridge
let mesh_bridge = EventMeshBridge::new().with_mesh(Arc::new(flow_handle_adapter));
mesh_bridge.emit_written(&uri, version, Some(&agent_id));
// 跨进程由 uwu_nats_bridge 订阅 mesh 主题桥接到 NATS
```

---

## 6. URI 寻址约定（必须遵守）

context-db 用 `uwu://` URI 统一寻址，所有 bridge 都按这个约定落盘：

```
uwu://{tenant}/agent/{id}/memories/{class}/{entry}   # 8 类记忆
uwu://{tenant}/agent/{id}/state/{short|mid|long}/     # State 快照
uwu://{tenant}/agent/{id}/persona/relations/          # Persona 关系
uwu://{tenant}/agent/{id}/metacog/pred_errors/        # 校准冷归档
uwu://{tenant}/wiki/{space}/{doc}/                    # 协作知识库
uwu://{tenant}/sessions/{id}/archive/{n}/             # 会话归档
```

`StateSnapshot::dir_uri` / `MetacogBridge::pred_errors_dir` 等辅助方法已封装好路径拼接，不要手拼字符串。

---

## 7. 常见坑

| 坑 | 规避 |
|----|------|
| 在 `process_turn` 热路径每轮都调 `log_pred_error` | 错。只在 `CalibrationHistory` evict 时调用，否则破坏"热路径零 IO"约束 |
| `Session` 直接 `use` `PgContextStore` 具体类型 | 错。`Session` 只依赖 `FsOps`/`ContentRepo` 等 trait，具体类型只在装配根出现 |
| `StateBridge::fork` 后忘记 `promote_fork` 或 `discard_fork` | 会泄漏 fork 分支，定期 GC 或用 RAII guard 包裹 |
| LLM 不可用时 `WriteGate` 阻塞写入 | 不会。设计上 LLM 失败默认 `Pass`（安全侧不阻塞） |
| 用 `ContextStore` 聚合 trait 做库内部依赖 | 错。`ContextStore` 仅应用层用；库内部只用 `FsOps`/`ContentRepo` 窄端口 |
| 没跑 m3 集成测试就上 PG | 先用 `MemoryContextStore` 跑通 m3/l4_l5 测试，再切 PG |

---

## 8. 接入清单（Checklist）

- [ ] `agent-session/Cargo.toml` 添加 `agent-context-db` + `agent-context-db-core` + `agent-context-db-version` 依赖
- [ ] 在装配根构造 `ContextDbService`（测试用 `MemoryContextStore`，生产用 `PgContextStore`）
- [ ] 构造五个 bridge：`StateBridge` / `MetacogBridge` / `CharacterConstraint`+`WriteGate` / `EventMeshBridge` / `ReactionLearner`
- [ ] `Session` struct 加 `context_db: Option<ContextDbHandles>` 字段
- [ ] `process_turn` 主循环挂载点：fork 推演（Step 4）、写入约束（Step 5）、冷归档（Step 6）、State checkpoint（Step 7）
- [ ] sidecar consolidator 接入 `ReactionLearner::induce_rules`
- [ ] 跑通 `cargo test -p agent-context-db --test m3_integration`
- [ ] 加端到端测试：带 `context_db` 的 `Session` 跑 `process_turn`，断言 `state_bridge.load` 读回 checkpoint
- [ ] 生产切换：`MemoryContextStore` → `PgContextStore` + `UwuVectorIndex`，跑 migrations

---

---

## 9. 巩固 / 记忆 / 学习：通用语义管线接入（正交于五维）

> 这一节回答你的问题：**context-db 要通用**。巩固/记忆/学习不是五维 bridge 的一部分，而是一条**与五维正交的独立语义处理管线**。它由 `context-db-session` + `context-db-parse` + `context-db-compressor` 三个通用 crate 组成，**零 uwu 依赖**，任何 Agent 框架都能用。接入 engine 时只需在装配根把这条管线串到 `Session` 的 turn 末尾 + sidecar。

### 9.1 这条管线是什么（先理清三条独立链路）

很多人把"巩固/记忆/学习"混为一谈，但仓库里它们是**三条正交链路**，接入方式完全不同：

| 链路 | 触发时机 | 作用 | 对应 crate | 是否通用 |
|------|---------|------|-----------|---------|
| **A. 会话压缩（巩固）** | 对话窗口满 / turn 结束 | 归档消息 → 提取记忆 → 去重 → 生成 L0/L1 → 写 memory_diff | `context-db-session` + `context-db-parse` + `context-db-compressor` | ✅ 通用，零 uwu 依赖 |
| **B. Episode 巩固（学习触发）** | Episode 完成 | LearnTrigger 评估 → 提取 Skill → Guard 检查 → Memory 持久化 | `agent-sidecar-consolidator` + `agent-learning` + `agent-guard` | ⚠️ uwu 专用，绑定五维 |
| **C. 记忆检索** | process_turn 的 Memory 阶段 | 意图分析 → 向量召回 → grep/read → Rerank → 幻觉检测 | `context-db-retrieve` | ✅ 通用 |

**关键认知**：链路 A 和链路 B **不是同一件事**。
- 链路 A 是**通用语义压缩**：把对话窗口冷归档 + 结构化为 8 类记忆，输出到 context-db 的 `uwu://.../memories/` 目录，任何框架都能用。
- 链路 B 是 **uwu 专属的 Skill 提取**：消费 `agent_learning::Episode`，跑 `LearnTrigger` 条件评估，产出 `SkillVersion`，绑 State fork 沙盒验证。它**消费**链路 A 产出的归档作为输入之一。

仓库 ROADMAP 里 `context-db-compressor` 的注释写得很明确："**替代 `agent-sidecar-consolidator` 的独立进程模式，内嵌为 in-process worker**"。也就是说，**链路 A 的 `TokioSemanticQueue` 是链路 B sidecar 的通用化替代**——但只替代"语义处理队列"那部分，`LearnTrigger` + `Guard` 的 Skill 提取逻辑仍由链路 B 保留。两者可以共存：A 做通用记忆提取，B 做 Skill 进化。

### 9.2 语义管线三层架构

```
┌─────────────────────────────────────────────────────────────────┐
│  Session::process_turn 末尾 / 窗口满时                           │
│    └─ enqueue SemanticTask::ExtractMemories { archive, session } │
└──────────────────────────┬──────────────────────────────────────┘
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│  L6  context-db-compressor  (TokioSemanticQueue, in-process)     │
│    tokio mpsc 无界通道 + spawn_worker                            │
│    任务类型：GenerateAbstract / ExtractMemories / Deduplicate /  │
│             ExtractTrajectory / InduceExperience / AggregateUp   │
└──────────────────────────┬──────────────────────────────────────┘
                           ▼ worker dequeue
┌─────────────────────────────────────────────────────────────────┐
│  L4  context-db-session   (SessionCompressorImpl, 两阶段 commit) │
│    Phase1 同步：归档 messages.jsonl → 返回 task_id               │
│    Phase2 异步：语义处理 → memory_diff.json → .done 标记          │
└──────────────────────────┬──────────────────────────────────────┘
                           ▼ 调用注入的 shim
┌─────────────────────────────────────────────────────────────────┐
│  L5  context-db-parse     (MemoryExtractorImpl + SemanticImpl)   │
│    MemoryExtractor: 8 类分类 + LLM 去重 (Create/Merge/Skip)      │
│    SemanticProcessor: L0 摘要 + L1 概览 + 自底向上聚合           │
│    TrajectoryExtractor: 会话→Trajectory，多轨迹→Experience       │
└──────────────────────────┬──────────────────────────────────────┘
                           ▼ 写入窄端口 ContentRepo
┌─────────────────────────────────────────────────────────────────┐
│  L1  context-db-core      (FsOps / ContentRepo 窄端口)           │
│    uwu://.../sessions/{id}/archive/{n}/messages.jsonl            │
│    uwu://.../memories/{class}/{entry}                            │
│    uwu://.../sessions/{id}/archive/{n}/memory_diff.json          │
└─────────────────────────────────────────────────────────────────┘
```

> **通用性体现**：L4/L5/L6 三个 crate 全部**零 uwu 依赖**，只依赖 core 的窄端口 trait。把 `Session` 换成任何其他框架的对话对象，这条管线照常工作。

### 9.3 接入步骤（在五维 bridge 之外额外做）

#### Step A：加依赖

```toml
[dependencies]
agent-context-db-session = { path = "../../pkg/uwu_context_db/context-db-session" }
agent-context-db-parse = { path = "../../pkg/uwu_context_db/context-db-parse" }
agent-context-db-compressor = { path = "../../pkg/uwu_context_db/context-db-compressor" }
```

#### Step B：构造三层组件并注入 LlmClient

```rust
use agent_context_db_session::{SessionCompressorImpl, SessionHandle, SessionMessage, Role};
use agent_context_db_parse::{MemoryExtractorImpl, SemanticProcessorImpl};
use agent_context_db_compressor::{TokioSemanticQueue, SemanticTask, SemanticQueue};
use std::sync::Arc;

// L5 实现注入 LlmClient（HttpLlmClient 或 MockLlmClient）
let extractor = Arc::new(MemoryExtractorImpl::new(store.clone(), llm.clone()));
let semantic = Arc::new(SemanticProcessorImpl::new(store.clone(), llm.clone()));

// L4 压缩器注入 ContentRepo 窄端口
let compressor = Arc::new(SessionCompressorImpl::new(store.clone()));

// L6 异步队列（in-process worker，替代 sidecar 独立进程）
let queue = Arc::new(TokioSemanticQueue::new());
```

#### Step C：启动 worker 消费 SemanticTask

worker 里按任务类型分派到 L5 的 extractor / semantic，这是接管的**核心逻辑**：

```rust
let q = queue.clone();
let worker = TokitoSemanticQueue::spawn_worker(&q, move |id, task| {
    let extractor = extractor.clone();
    let semantic = semantic.clone();
    let compressor = compressor.clone();
    async move {
        match task {
            SemanticTask::ExtractMemories { archive, session } => {
                // ★ 完整两阶段 commit（run_full_compression 编排）
                let result = agent_context_db_session::run_full_compression(
                    &compressor, &*extractor, &*semantic, &session,
                ).await;
                match result {
                    Ok(_) => TaskOutcome::Success,
                    Err(e) => TaskOutcome::PartialFailure(e.to_string()),
                }
            }
            SemanticTask::GenerateAbstract(uri) => {
                match semantic.generate_abstract(&uri).await {
                    Ok(_) => TaskOutcome::Success,
                    Err(e) => TaskOutcome::Failure(e.to_string()),
                }
            }
            SemanticTask::AggregateUpward(uri) => {
                match semantic.aggregate_upward(&uri).await {
                    Ok(_) => TaskOutcome::Success,
                    Err(e) => TaskOutcome::Failure(e.to_string()),
                }
            }
            SemanticTask::DeduplicateMemories(candidates) => {
                match extractor.deduplicate(candidates).await {
                    Ok(_) => TaskOutcome::Success,
                    Err(e) => TaskOutcome::Failure(e.to_string()),
                }
            }
            SemanticTask::ExtractTrajectory(uri) => {
                // TrajectoryExtractorImpl 同样注入 LlmClient
                // ... TrajectoryExtractor::extract_trajectory(&uri)
                TaskOutcome::Success
            }
            SemanticTask::InduceExperience(uris) => {
                // 多轨迹 → Experience 归纳
                TaskOutcome::Success
            }
            // ... 其他变体
            _ => TaskOutcome::Failure("unhandled".into()),
        }
    }
});
```

#### Step D：在 `Session::process_turn` 末尾入队

**不要在热路径同步跑语义处理**。turn 结束时只入队，worker 异步消费：

```rust
// process_turn 末尾，在 State checkpoint 之后
if let Some(ref cdb) = self.context_db {
    let session_handle = SessionHandle {
        session_id: self.session_id.0,
        user_id: self.user_id.clone(),
        agent_id: self.agent_id.to_string(),
        messages: self.history.recent_messages(/* 窗口大小 */),
        compression_index: self.compression_index,
        archive_dir: ContextUri::parse(&format!(
            "uwu://default/sessions/{}/archive", self.session_id.0
        )).unwrap(),
    };
    cdb.semantic_queue.enqueue(SemanticTask::ExtractMemories {
        archive: session_handle.archive_dir.clone(),
        session: Box::new(session_handle),
    }).await?;
    self.compression_index += 1;
}
```

### 9.4 记忆检索：替换 `MemoryFacade` 的内存检索

当前 `Session::run_flowgraph` 第 2 步用 `MemoryFacade::retrieve`（纯内存向量）。接入 context-db 后可**叠加** `context-db-retrieve` 的分层检索：

```rust
use agent_context_db_retrieve::{HierarchicalRetrieverImpl, RetrieveContext};

// 装配根构造
let retriever = Arc::new(HierarchicalRetrieverImpl::new(
    store.clone(),         // FsOps 只读端口
    Some(vector_index),    // 可选 VectorIndex（Qdrant/Pgvector）
    Some(llm.clone()),     // 可选 LLM 意图分析
));

// Session 持有
pub struct ContextDbHandles {
    // ... 五维 bridge ...
    pub retriever: Arc<HierarchicalRetrieverImpl>,
    pub semantic_queue: Arc<TokioSemanticQueue>,
}

// run_flowgraph 第 2 步改为
let ctx = RetrieveContext {
    agent_id: Some(self.agent_id.to_string()),
    prefer_level: ContentLevel::L1,
    ..Default::default()
};
let result = cdb.retriever.retrieve(&ctx_desc.description, &ctx).await?;
let context_str = result.hits.iter()
    .map(|h| h.content.as_str())
    .collect::<Vec<_>>().join("\n");
```

> **双轨策略**：热路径先用 `MemoryFacade`（内存，零 IO）做快速召回；miss 时 fallback 到 `HierarchicalRetriever`（context-db，持久化 + 向量）。这样既保住热路径性能，又拿到冷存的完整性。

### 9.5 与现有 `agent-sidecar-consolidator` 的关系

这是接入时最容易混淆的点。下表说清两者职责：

| 维度 | `agent-sidecar-consolidator`（链路 B） | `context-db-compressor`（链路 A） |
|------|---------------------------------------|----------------------------------|
| 进程模式 | 独立进程 / NATS JetStream 订阅 | in-process tokio worker |
| 输入 | `agent_learning::Episode`（uwu 专属） | `SessionHandle`（通用） |
| 核心逻辑 | `LearnTrigger` 条件评估 → `SkillVersion` 提取 → `Guard` egress 检查 | 两阶段 commit → MemoryExtractor 8 类分类 → 去重 → L0/L1 生成 |
| 输出 | Skill 注册到 `SkillRegistry` + Memory 持久化 | `memories/{class}/` 目录 + `memory_diff.json` + `.done` 标记 |
| 通用性 | uwu 专属（绑 agent-learning/guard/state） | 通用（零 uwu 依赖） |
| 是否要替换 | **保留**——Skill 进化逻辑无可替代 | **新增**——做通用记忆提取，反哺 MemoryFacade |

**推荐：两者共存。**
- 链路 A（context-db-compressor）每轮 turn 末尾入队，做对话归档 + 8 类记忆提取，产出结构化记忆到 `uwu://.../memories/`。
- 链路 B（sidecar consolidator）订阅 `Episode` 流，在 A 产出的归档基础上做 Skill 提取。可以把 A 产出的 `Trajectory` / `Experience`（由 L5 `TrajectoryExtractor` 生成）作为 B 的 `Episode` 输入源之一。

### 9.6 学习链路：`agent-learning` + `ReactionLearner` 的衔接

学习分两层，接入点不同：

| 层 | 触发 | 产出 | 接入点 |
|----|------|------|--------|
| **显式 Skill 提取** | `Episode` 完成 + `LearnTrigger` 命中 | `SkillVersion` 注册到 `SkillRegistry` | sidecar-consolidator（链路 B） |
| **隐式 Reaction 规则归纳** | sidecar 异步扫描 experiences 目录 | `NewRule` 下发到 `ReactionLayer` | `agent-context-db::ReactionLearner`（见 §5.4） |

`ReactionLearner` 已经在五维 bridge 那节讲过。它消费的 `experiences/` 目录正是链路 A 的 `TrajectoryExtractor::induce_experience` 产出的。所以**完整学习闭环**是：

```
turn 末尾 → 链路A入队 → ExtractMemories + ExtractTrajectory
         → InduceExperience（多轨迹归纳）→ experiences/ 目录
         → ReactionLearner::induce_rules 扫描 experiences/ → NewRule
         → EventMeshBridge 下发 → ReactionLayer 注册新规则
         → 下一轮 turn 的 Reaction.intercept 可能命中新规则
```

### 9.7 通用性检查清单（确保 context-db 不绑死 uwu）

接入时用这张表自检，保证通用性：

| 检查项 | 要求 |
|--------|------|
| L4/L5/L6 是否 `use` 了 `agent-*` crate | ❌ 禁止。只能依赖 `agent-context-db-core` |
| `SessionCompressor` trait 的参数是否含 uwu 类型 | ❌ 只能用 `SessionHandle`（通用） |
| `MemoryExtractorShim` / `SemanticProcessorShim` 是否绕过 parse crate 直接依赖 uwu | ❌ trait shim 就是为了隔离 |
| LLM 调用是否走 `LlmClient` trait | ✅ 必须走 trait，不能直接 `use reqwest` |
| 存储是否走 `FsOps`/`ContentRepo` 窄端口 | ✅ 禁止依赖 `PgContextStore` 具体类型 |
| 五维 bridge（agent-context-db）是否被 L4/L5/L6 引用 | ❌ 方向反过来：uwu 层依赖通用层，不是反之 |

### 9.8 接入清单（巩固/记忆/学习部分）

- [ ] `agent-session/Cargo.toml` 加 `context-db-session` / `context-db-parse` / `context-db-compressor` / `context-db-retrieve` 依赖
- [ ] 装配根构造 `SessionCompressorImpl` + `MemoryExtractorImpl` + `SemanticProcessorImpl` + `HierarchicalRetrieverImpl`，全部注入 `store` + `llm`
- [ ] 构造 `TokioSemanticQueue` 并 `spawn_worker`，handler 内分派 `SemanticTask` 变体到 L5 实现
- [ ] `ContextDbHandles` 增加 `semantic_queue` + `retriever` 字段
- [ ] `process_turn` 末尾：enqueue `SemanticTask::ExtractMemories`（异步，不阻塞响应）
- [ ] `run_flowgraph` 第 2 步：叠加 `HierarchicalRetriever` 冷检索（MemoryFacade miss 时 fallback）
- [ ] sidecar-consolidator 保留：订阅 Episode 流做 Skill 提取，输入可包含链路 A 产出的 Trajectory
- [ ] `ReactionLearner` 定期扫 `experiences/` 目录，产出的 `NewRule` 通过 `EventMeshBridge` 下发
- [ ] 跑 `cargo test -p agent-context-db-compressor` 验证队列
- [ ] 跑 `cargo test -p agent-context-db-session` 验证两阶段 commit
- [ ] 端到端：连续 10 轮 turn 后，断言 `uwu://.../memories/` 下有提取出的记忆条目

---

## 10. 全模块接入盘点（破坏性更新允许）

> 这一节回答："**其余模块哪些还需要接入**"。我把仓库全部 22 个 agent crate + 9 个 pkg crate + 10 个 context-db 子 crate 逐一过完，按"是否需要接入 context-db / 接入方式 / 是否破坏性重构"分类。允许破坏性更新，所以 recommendations 比保守接入更激进。

### 10.1 接入分级总表

| crate | 现状 | 是否接入 | 重构幅度 | 说明 |
|-------|------|---------|---------|------|
| **agent-session** | 无 cdb 字段 | ✅ 必须 | 🔴 破坏性 | 装配根，加 `ContextDbHandles`，主循环 6 个挂载点 |
| **agent-state** | 纯内存 | ✅ 必须 | 🔴 破坏性 | checkpoint/fork 改走 `StateBridge`，派生标量留内存 |
| **agent-metacognition** | 纯内存环形缓冲 | ✅ 必须 | 🟡 中等 | evict 落盘改走 `MetacogBridge`，热路径不变 |
| **agent-memory** | `MemoryFacade` 纯内存 | ✅ 必须 | 🔴 破坏性 | 叠加 `HierarchicalRetriever` 冷检索，双轨 |
| **agent-character** | `core_values` 内存 | ✅ 必须 | 🟡 中等 | 接 `CharacterConstraint` + `WriteGate` 写前置 |
| **agent-reaction** | 规则内存 | ✅ 应接入 | 🟡 中等 | 接 `ReactionLearner` 产出 `NewRule` 下发 |
| **agent-learning** | `LearnTrigger` 内存 | ✅ 应接入 | 🟡 中等 | `Episode` 输入源接 context-db 的 Trajectory/Experience |
| **agent-sidecar-consolidator** | 独立进程，绑 learning/guard | ✅ 重构 | 🔴 破坏性 | 语义队列改用 `TokioSemanticQueue`，保留 Skill 提取逻辑 |
| **agent-sidecar-monitor** | 独立进程，读内存快照 | ✅ 应接入 | 🟡 中等 | 异常数据源改读 `MetacogBridge` 冷归档 |
| **agent-wiki** | `MemoryWikiStore` 内存 | ✅ 必须 | 🔴 破坏性 | 换 `context-db-wiki::ContextDbWikiStorage` |
| **agent-persona** | MVCC 内存 | ✅ 应接入 | 🟡 中等 | 关系图/履历落 `uwu://.../persona/` 目录 |
| **agent-task** | 纯内存 `active_tasks` | ✅ 应接入 | 🟡 中等 | Task/SubtaskDAG 持久化到 `uwu://.../tasks/` |
| **agent-collaboration** | `AgentRegistry` 内存 | ⚪ 可选 | 🟢 小 | 跨 Agent 委派记录可归档，非必须 |
| **agent-mesh** | 事件网格 | ⚪ 不接入 | — | 是传输层，context-db 通过 `EventMeshBridge` 向它发事件，不反向依赖 |
| **agent-perception** | 输入解析 + PII | ⚪ 不接入 | — | 无状态解析，不需要持久化 |
| **agent-reasoning** | fork 沙盒 + ToT | ⚪ 间接 | 🟢 小 | fork 走 `StateBridge`，reasoning 本身不接 cdb |
| **agent-execution** | MCP 调用 | ⚪ 不接入 | — | 无状态执行，Guard 在自己层做 |
| **agent-guard** | 五层闸门 | ⚪ 不接入 | — | 编译期注册，不持久化；与 `WriteGate` 是两层不同闸门 |
| **agent-core** | FlowGraph + FlowEngine | ⚪ 不接入 | — | 编排层，无状态 |
| **agent-tools** | MCP 协议类型 | ⚪ 不接入 | — | 纯类型定义 |
| **agent-uncertainty** | Bayesian 估计 | ✅ 应接入 | 🟡 中等 | `BetaBelief` 观测历史冷归档，热路径只留当前 belief |
| **agent-types-core / -ext** | 基础类型 | ⚪ 不接入 | — | 类型定义层 |
| **uwu_event_mesh** | 事件网格基础设施 | ⚪ 不接入 | — | 传输层 |
| **uwu_visual_script** | 可视化脚本引擎 | ⚪ 不接入 | — | 执行引擎 |
| **uwu_wasm** | WASM 沙箱 | ⚪ 间接 | 🟢 小 | `agent-context-db::WasmSandbox` 已封装统计/聚类模块 |
| **uwu_database** | SQL + 向量存储 | ⚪ 间接 | — | `context-db-storage` 已适配，上层不直接用 |
| **uwu_crdt** | CRDT 类型 | ⚪ 间接 | 🟢 小 | `context-db-version::CrdtMerger` 已用 |
| **uwu_nats_bridge** | NATS 桥接 | ⚪ 不接入 | — | 跨进程传输 |
| **uwu_logger** | 日志 | ⚪ 不接入 | — | 日志 |
| **uwu_wiki** | wiki-core 子域 | ✅ 必须 | 🔴 破坏性 | 通过 `context-db-wiki` 适配器接入 |

> ✅ 必须 = 不接入会丢数据；✅ 应接入 = 接入收益显著；⚪ 不接入 = 无状态或非数据层；⚪ 间接 = 通过别的 crate 间接接。

### 10.2 必须接入的模块（破坏性重构）

#### (1) agent-state —— 派生/事实层分离重构

**现状问题**：`AgentState` 纯内存，`checkpoint()` 只返回 `StateCheckpoint`（内存句柄），进程崩溃即丢失。

**破坏性重构**：
```rust
// 旧：纯内存 checkpoint
pub fn checkpoint(&self) -> StateCheckpoint { ... }

// 新：checkpoint 同时落 context-db
pub async fn checkpoint_persistent(
    &mut self,
    bridge: &StateBridge<S, V>,
    scope: StateScope,
    tenant: TenantId,
) -> Result<MvccVersion> {
    let snap = StateSnapshot::new(
        self.agent_id.clone(),
        scope,
        self.seq_next(),
        self.parent_uri.clone(),
        self.accumulated_pred_error,  // 派生标量
        serde_json::to_value(&self)?,  // 事实层
    );
    bridge.checkpoint(&self.agent_id, scope, &snap, tenant).await
}
```

**保留**：`accumulated_pred_error: f32` 留内存做热路径零 IO 读取；`fork()` 内存沙盒逻辑保留，但 `promote_fork` 改走 `StateBridge::promote_fork`。

#### (2) agent-memory —— 双轨检索重构

**现状问题**：`MemoryFacade` 只检索内存向量，进程重启记忆全丢。

**破坏性重构**：`MemoryFacade` 拆成"热缓存 + 冷检索"双轨：
```rust
pub struct MemoryFacade {
    hot: UnifiedMemory,                           // 内存热态（派生层）
    cold: Option<Arc<dyn HierarchicalRetriever>>, // context-db 冷检索（事实层）
}

impl MemoryFacade {
    pub async fn retrieve(&mut self, query: &str) -> RetrievedMemories {
        // 1. 热路径：内存向量召回（零 IO）
        let hot_hits = self.hot.retrieve(&RetrievalIntent::simple(query));
        if hot_hits.len() >= self.min_hits {
            return RetrievedMemories::new(hot_hits);
        }
        // 2. 冷路径：context-db 分层检索
        if let Some(cold) = &self.cold {
            let ctx = RetrieveContext::default();
            let cold_hits = cold.retrieve(query, &ctx).await?;
            // 回填热缓存
            for h in &cold_hits { self.hot.upsert(h.into()); }
            return RetrievedMemories::from_cold(cold_hits);
        }
        RetrievedMemories::new(hot_hits)
    }
}
```

**破坏性点**：`retrieve` 从同步变 `async`，所有调用方（`Session::run_flowgraph`）签名要改。

#### (3) agent-wiki —— 存储后端替换

**现状问题**：`Session.wiki: Option<MemoryWikiStore>`，重启丢失。

**重构**：`MemoryWikiStore` 换成 `context-db-wiki::ContextDbWikiStorage`：
```rust
// 装配根
let wiki_storage = ContextDbWikiStorage::new(
    vector_index,    // context-db 索引层
    pg_doc_store,    // 其余 6 端口由 PG 适配器注入
    pg_op_log,
    pg_text_index,
    pg_link_store,
    pg_blob_store,
    pg_version_store,
);
// Session.wiki 字段类型改为 Arc<dyn WikiStorage>
```

`uwu_wiki`（wiki-core 子域）同步重构，7 个存储端口全部走 context-db。

#### (4) agent-sidecar-consolidator —— 语义队列替换

**现状问题**：独立进程模式 + NATS 订阅，与 `context-db-compressor` 的 in-process worker 职责重叠。

**破坏性重构**：拆成两层：
- **语义处理层**：删除独立进程模式，改用 `TokioSemanticQueue` in-process worker（链路 A，见 §9）。
- **Skill 提取层**：保留 `Consolidator` 的 `LearnTrigger` + `Guard` 逻辑，但输入源改为从 context-db 读 `Trajectory`/`Experience`：
```rust
// 旧：从 NATS 订阅 Episode
pub async fn run_with_nats(&mut self, nats_url, correlation_id) { ... }

// 新：从 context-db 拉 Trajectory
pub async fn run_with_context_db(&mut self, cdb: &ContextDbHandles) {
    let experiences_dir = ContextUri::parse("uwu://default/agent/a1/experiences")?;
    let trajectories = cdb.fs_ops.ls(&experiences_dir).await?;
    for traj_uri in trajectories {
        let traj: Trajectory = cdb.read(&traj_uri).await?;
        let episode = Episode::from_trajectory(&traj);
        self.process(&episode).await;
    }
}
```

`run_with_nats` 保留为 feature 开关（跨进程场景仍需 NATS）。

### 10.3 应接入的模块（中等重构）

#### (5) agent-persona —— 关系图持久化

**现状**：`Persona::update_relationship` 只改内存，MVCC version 是内存计数器。

**接入**：关系图变更落 `uwu://.../persona/relations/`，履历落 `uwu://.../persona/history/`：
```rust
pub async fn update_relationship_persistent(
    &mut self,
    peer: AgentId,
    trust_delta: f32,
    store: &dyn ContentRepo,
) -> Result<()> {
    self.version += 1;
    self.relationships.adjust_trust(peer.clone(), trust_delta);
    let uri = ContextUri(format!("uwu://default/agent/{}/persona/relations/{}", self.agent_id, peer));
    let entry = ContextEntry::new_text(uri, tenant, &format!("trust={}", trust_delta));
    store.write(entry).await?;
    Ok(())
}
```

#### (6) agent-task —— 任务持久化

**现状**：`Session.active_tasks: Vec<Task>` 纯内存，进程重启任务丢失。

**接入**：Task + SubtaskDAG 落 `uwu://.../tasks/{task_id}/`：
```
uwu://default/agent/{id}/tasks/{task_id}/manifest.json
uwu://default/agent/{id}/tasks/{task_id}/dag.json
uwu://default/agent/{id}/tasks/{task_id}/status
```
`SubtaskScheduler::progress` 改为读 context-db 的 DAG 文件。

#### (7) agent-uncertainty —— 信念历史冷归档

**现状**：`BetaBelief` 只在内存累积观测，重启归零。

**接入**：`BetaBelief` 的 `total_observations` + `alpha/beta` 参数定期落 `uwu://.../uncertainty/{belief_name}/`。热路径只留当前 belief 参数（标量，零 IO），历史观测冷归档供事后分析。

#### (8) agent-sidecar-monitor —— 数据源切换

**现状**：`AnomalyDetector` 从 channel 收预测误差值。

**接入**：异常检测的数据源改为 `MetacogBridge::retrieve_calibration`（冷归档 + 热样本合并）。这样 monitor 独立进程也能跑（读 context-db 快照），不依赖主进程 channel。

### 10.4 不接入的模块（明确理由）

| crate | 不接入原因 |
|-------|-----------|
| **agent-perception** | 无状态解析器（文本→ContextDescriptor + PII 检测），无数据要持久化 |
| **agent-reasoning** | fork 推演通过 `StateBridge` 间接接入，reasoning 本身是纯计算 |
| **agent-execution** | MCP 调用无状态，Guard 在自己层做，执行结果由 memory 侧持久化 |
| **agent-guard** | 五层闸门编译期注册，不持久化；`WriteGate`（context-db 层）与 `GuardLayer`（engine 层）是两层不同闸门，各司其职 |
| **agent-core** | FlowGraph 编排拓扑，纯内存声明式结构 |
| **agent-mesh** | 事件传输层，context-db 通过 `EventMeshBridge` 向它**单向发**事件，不反向依赖 |
| **agent-tools** | MCP 协议类型定义，无状态 |
| **agent-types-core / -ext** | 基础类型层 |
| **uwu_event_mesh / uwu_visual_script / uwu_wasm / uwu_database / uwu_crdt / uwu_nats_bridge / uwu_logger** | 基础设施层，context-db 已通过适配器接入其中需要的部分（database/vector/crdt） |

### 10.5 重构顺序（依赖拓扑序）

破坏性重构要按依赖拓扑从底层往上做，避免反复改：

```
第1层（基础设施适配）
  └─ context-db-storage / context-db-wiki  ← 已就绪，无需改

第2层（五维落盘）
  ├─ agent-state        ← StateBridge 接入
  ├─ agent-metacognition ← MetacogBridge 接入
  ├─ agent-character    ← CharacterConstraint 接入
  ├─ agent-persona      ← 关系图持久化
  └─ agent-memory       ← 双轨检索（async 化）

第3层（语义管线）
  ├─ agent-session      ← ContextDbHandles + 主循环挂载
  ├─ context-db-compressor worker 启动
  └─ agent-reaction     ← ReactionLearner 下发

第4层（学习/监控）
  ├─ agent-learning     ← Episode 输入源切换
  ├─ agent-sidecar-consolidator ← 语义队列替换
  ├─ agent-sidecar-monitor     ← 数据源切换
  └─ agent-uncertainty  ← 信念归档

第5层（协作域）
  ├─ agent-task         ← 任务持久化
  ├─ agent-wiki / uwu_wiki ← 存储后端替换
  └─ agent-collaboration ← 委派记录归档（可选）
```

### 10.6 破坏性变更清单（Breaking Changes）

重构会引入以下破坏性变更，需要全仓库批量改：

| 变更 | 影响范围 | 迁移方式 |
|------|---------|---------|
| `MemoryFacade::retrieve` 同步→async | 所有调用方 | 加 `.await`，调用方函数签名加 `async` |
| `Session` 新增 `context_db` 字段 | 所有 `Session::new` 构造点 | 用 builder 模式，`context_db` 设 `Option` |
| `Session.wiki` 类型 `MemoryWikiStore`→`Arc<dyn WikiStorage>` | wiki 相关方法 | trait object 替换具体类型 |
| `AgentState::checkpoint` 加 persistent 版本 | 所有 checkpoint 调用点 | 旧方法保留 deprecated，新方法 `checkpoint_persistent` |
| `Consolidator::run_with_nats` 改为 feature 开关 | sidecar 启动入口 | 默认 in-process，`nats` feature 开启跨进程 |
| `Session` 加 `compression_index` 字段 | Session 构造 | Default 实现 |
| `Persona::update_relationship` 加 persistent 版本 | 协作相关调用 | 旧方法保留，新方法 `_persistent` |

### 10.7 全量接入清单（Checklist）

**第2层 五维落盘**
- [ ] `agent-state`：`checkpoint_persistent` / `fork` 走 `StateBridge`
- [ ] `agent-metacognition`：evict 走 `MetacogBridge::log_pred_error`
- [ ] `agent-character`：write 前置 `CharacterConstraint::check_write`
- [ ] `agent-persona`：关系图/履历落 `uwu://.../persona/`
- [ ] `agent-memory`：`retrieve` async 化 + 冷检索双轨

**第3层 语义管线**
- [ ] `agent-session`：`ContextDbHandles` 字段 + 主循环 6 挂载点
- [ ] `TokioSemanticQueue` worker 启动 + `SemanticTask` 分派
- [ ] `agent-reaction`：`ReactionLearner::induce_rules` 下发 `NewRule`

**第4层 学习/监控**
- [ ] `agent-learning`：`Episode` 从 `Trajectory` 转换
- [ ] `agent-sidecar-consolidator`：语义队列替换为 `TokioSemanticQueue`，NATS 改 feature
- [ ] `agent-sidecar-monitor`：数据源改 `MetacogBridge::retrieve_calibration`
- [ ] `agent-uncertainty`：`BetaBelief` 参数冷归档

**第5层 协作域**
- [ ] `agent-task`：Task/DAG 落 `uwu://.../tasks/`
- [ ] `agent-wiki` + `uwu_wiki`：换 `ContextDbWikiStorage`
- [ ] `agent-collaboration`：委派记录归档（可选）

**验证**
- [ ] `cargo test --workspace` 全绿
- [ ] 端到端：进程重启后 State/Memory/Task/Wiki 不丢失
- [ ] 端到端：10 轮 turn 后 `uwu://.../memories/` 有提取记忆
- [ ] 端到端：`ReactionLearner` 产出 `NewRule` 并被 `ReactionLayer` 命中

---

## 11. agent-wiki 该不该替换为 context-db / uwu_wiki？

> 这个问题问得关键,因为仓库里有**三个**wiki 相关实体,不先理清谁是谁就会替换错。

### 11.1 三个 wiki 实体的真实关系

仓库里有三个 wiki 相关的 crate,**它们不是同一层的东西**:

| crate | 位置 | 实质 | 数据模型 | 存储 |
|-------|------|------|---------|------|
| **`agent-wiki`** | `crates/agent-wiki` | Agent 域的**简易 wiki 适配层** | `WikiPage`(扁平 KV: title/content/category/tags) | `MemoryWikiStore`(HashMap) / `DatabaseWikiStore`(feature) |
| **`uwu_wiki`** | `pkg/uwu_wiki` | 通用**结构化知识库子系统**(7 个子 crate) | `Block` 树 + `Document` + `Op` 日志 | `WikiStorage` 7 端口 trait(零实现) |
| **`context-db-wiki`** | `pkg/uwu_context_db/context-db-wiki` | **桥接层**:把 uwu_wiki 的 7 端口适配到 context-db 的 PG+Qdrant | 不持有模型,只翻译 | `ContextDbWikiStorage`(vector_store 走 context-db 索引层,其余 6 端口由 PG 适配器注入) |

**关键事实**:`agent-session` 现在 `use agent_wiki::{MemoryWikiStore, WikiPage, WikiRepo}`,用的是 `agent-wiki` 的**简易适配层**,**根本没碰 `uwu_wiki` 的 Block 引擎**。也就是说,`agent-wiki` 和 `uwu_wiki` 目前是两套独立的东西,只是名字像。

### 11.2 数据模型对比(差距很大)

```
agent-wiki (扁平):                    uwu_wiki (结构化):
┌─────────────────────┐              ┌─────────────────────────────┐
│ WikiPage            │              │ Document                    │
│  .title: String     │              │  .root: Block               │
│  .content: String   │   vs         │    .children: Vec<Block>    │
│  .category: String  │              │      .content: BlockContent │
│  .tags: Vec<String> │              │      .embedding: Vec<f32>   │
│  .version: u64      │              │  .op_log: Vec<Op>           │
└─────────────────────┘              │  .version_history           │
                                     │  + 7 端口:doc/op/text/      │
                                     │    link/blob/vector/version │
                                     └─────────────────────────────┘
```

`agent-wiki` 的 `WikiPage` 是扁平字符串;`uwu_wiki` 的 `Document` 是 Block 树 + Op 日志 + CRDT 协作 + 增量 embedding + LLM 工作流(Ingest/Query/Lint)。**功能量级差一个数量级**。

### 11.3 结论:不是"替换",是"分层归并"

你的直觉方向对,但措辞要精确。正确的做法**不是**把 `agent-wiki` 整个删掉换 `context-db`,而是分两步:

#### 步骤 1:`agent-wiki` 的存储后端 → `context-db-wiki`(必须做)

`agent-wiki` 现在的 `MemoryWikiStore`(HashMap)重启即丢。存储后端必须换。但**数据模型**(`WikiPage` 扁平结构)可以先保留,只换存储实现:

```rust
// 旧:agent-wiki 自带的内存存储
pub struct MemoryWikiStore { pages: HashMap<String, WikiPage> }

// 新:agent-wiki 的 WikiRepo trait 由 context-db 实现
pub struct ContextDbWikiRepo {
    store: Arc<dyn ContentRepo>,   // context-db 内容层
    fs: Arc<dyn FsOps>,            // context-db 只读寻址
}

#[async_trait]
impl WikiRepo for ContextDbWikiRepo {
    async fn save(&mut self, page: &WikiPage) -> Result<(), WikiRepoError> {
        let uri = ContextUri(format!("uwu://default/wiki/pages/{}", page.page_id));
        let entry = ContextEntry::new_text(uri, tenant, &serde_json::to_string(page)?);
        self.store.write(entry).await.map_err(|e| WikiRepoError::Storage(e.to_string()))
    }
    async fn search(&self, query: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        // 用 context-db-retrieve 的 HierarchicalRetriever 做向量召回
        // 或退化为 grep
        ...
    }
}
```

这样 `Session.wiki` 字段类型从 `Option<MemoryWikiStore>` 改为 `Option<Arc<dyn WikiRepo>>`,装配根注入 `ContextDbWikiRepo`。**`WikiPage` 模型不变,`Session::save_to_wiki`/`search_wiki` 签名不变**,影响面最小。

#### 步骤 2:`agent-wiki` 整体**降级为** `uwu_wiki` 的薄适配层(破坏性,但值得)

步骤 1 只是换了存储,但 `WikiPage` 扁平模型浪费了 `uwu_wiki` 的 Block 引擎能力。如果你要**真正用上**结构化知识库(增量 embedding / CRDT 协作 / LLM Ingest/Lint),应该把 `agent-wiki` 重构为 `uwu_wiki` 的薄适配:

```rust
// 重构后:agent-wiki 变成 uwu_wiki 的 Agent 域适配层
pub struct AgentWiki {
    space: WikiSpace,  // uwu_wiki::WikiSpace,注入 ContextDbWikiStorage
}

impl AgentWiki {
    pub async fn save_page(&self, title: &str, content: &str, category: &str, author: &str) -> Result<DocId> {
        // 把扁平 WikiPage 翻译成 Block 树
        let root = Block::new(BlockType::Paragraph, BlockContent::text(content), author);
        let mut doc = self.space.create_doc(title, root).await?;
        doc.metadata = serde_json::json!({"category": category});
        self.space.save_doc(&doc).await?;
        Ok(doc.id)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Document>> {
        // 走 uwu_wiki 的语义检索(向量 + 全文 + 反向链接)
        let emb = self.llm.embed(query).await?;
        self.space.search_semantic(emb, 10).await
    }
}
```

**破坏性点**:`Session.wiki` 字段类型从 `Option<MemoryWikiStore>` 改为 `Option<AgentWiki>`,`save_to_wiki`/`search_wiki` 返回类型从 `WikiPage` 改为 `Document`。所有调用方要改。

### 11.4 `uwu_wiki` 的存储怎么接 context-db

这一步由 `context-db-wiki` crate **已经做好了**。看 `context-db-wiki/src/lib.rs`:`ContextDbWikiStorage` 把 `uwu_wiki` 的 7 端口适配到 context-db:

```
uwu_wiki 7 端口              context-db 适配
─────────────────────────────────────────────────
VectorStore         ──▶  WikiVectorStoreAdapter ──▶ context-db VectorIndex (Qdrant)
DocStore            ──▶  PG 适配器(宿主注入)
OpLog               ──▶  PG 适配器
TextIndex           ──▶  PG 适配器
LinkStore           ──▶  PG 适配器
BlobStore           ──▶  PG 适配器
DocVersionStore     ──▶  PG 适配器
```

`vector_store` 由 `context-db-wiki` 自己桥接(走 context-db 索引层),其余 6 端口由装配根注入 PG 适配器。**`uwu_wiki` 子域完全不自持存储,真值源唯一(context-db 的 PG+Qdrant)**。

### 11.5 推荐方案(按破坏性递增三档)

| 档 | 方案 | 改动 | 收益 |
|----|------|------|------|
| 🟢 保守 | `agent-wiki` 存储换 `ContextDbWikiRepo`,`WikiPage` 模型保留 | 小,只换 `WikiRepo` 实现 | 持久化不丢,但浪费 Block 引擎 |
| 🟡 中等 | `agent-wiki` 重构为 `uwu_wiki` 薄适配层,`ContextDbWikiStorage` 注入 | 中,`Session.wiki` 类型变,`save/search` 签名变 | 拿到 Block 树 + 增量 embedding + 向量检索 |
| 🔴 激进 | **删除 `agent-wiki` crate**,`Session` 直接持有 `uwu_wiki::WikiSpace` | 大,删除整个 crate,所有 wiki 调用改走 `WikiSpace` API | 最干净,无中间适配层,但要迁移所有 `WikiPage` 用法 |

**我的建议:走 🟡 中等档**。理由:
- 🟢 保守档虽然改动小,但 `WikiPage` 扁平模型和 context-db 的 `uwu://` URI 树形寻址不匹配,长期是技术债。
- 🔴 激进档删除 `agent-wiki` 最干净,但 `agent-wiki` 有 `DatabaseWikiStore` (feature)和 `WikiPage` 的 MVCC 版本历史,这些在 `uwu_wiki` 里要重新对应到 `Document` + `DocVersionStore`,迁移成本高。
- 🟡 中等档保留 `agent-wiki` 作为 Agent 域适配层(它本来就是这定位),内部改用 `uwu_wiki::WikiSpace`,存储由 `ContextDbWikiStorage` 注入。这样 `uwu_wiki` 的 Block 引擎能力全开,`agent-wiki` 负责把 Agent 域概念(决策记录/经验/技能)映射成 Wiki Document。

### 11.6 回答你的原问题

> **agent wiki 是不是应该替换为 context db 也就是 uwu wiki?**

精确回答:
- **不是"替换为 context-db"** —— context-db 是存储层,不是 wiki 本身。wiki 的数据模型(Block/Document/Op)在 `uwu_wiki` 里,context-db 只负责存。
- **是"agent-wiki 的存储后端替换为 context-db"** —— 这一步必须做,`MemoryWikiStore` 重启丢数据。
- **进一步"agent-wiki 降级为 uwu_wiki 的适配层"** —— 推荐做,这样才能用上 Block 引擎 + 增量 embedding + CRDT 协作 + LLM 工作流。
- **最终链路**:`Session` → `agent-wiki::AgentWiki`(Agent 域适配) → `uwu_wiki::WikiSpace`(Block 引擎) → `context-db-wiki::ContextDbWikiStorage`(7 端口桥接) → `context-db-storage`(PG+Qdrant)。

```
Session.wiki: Option<AgentWiki>
                ↓
            AgentWiki (agent-wiki, 薄适配)
                ↓
            WikiSpace (uwu_wiki::wiki-core, Block 引擎)
                ↓
            ContextDbWikiStorage (context-db-wiki, 7 端口桥接)
                ↓
            PgContextStore + UwuVectorIndex (context-db-storage, PG+Qdrant)
```

### 11.7 接入清单(wiki 部分)

- [ ] 装配根构造 `ContextDbWikiStorage`(注入 `VectorIndex` + 6 个 PG 适配器)
- [ ] 装配根构造 `WikiSpace::new(space_id, Arc::new(context_db_wiki_storage))`
- [ ] `agent-wiki` 重构:`AgentWiki` 包装 `WikiSpace`,提供 `save_page`/`search` 等 Agent 域 API
- [ ] `Session.wiki` 字段类型改为 `Option<AgentWiki>`
- [ ] `Session::save_to_wiki` 内部:`WikiPage` → `Block` 树 → `WikiSpace::create_doc`
- [ ] `Session::search_wiki` 内部:走 `WikiSpace::search_semantic`(向量)+ `search_text`(全文)
- [ ] 迁移 `agent-wiki` 现有测试:`MemoryWikiStore` 测试改为 `AgentWiki` + `MemoryContextStore` 测试
- [ ] 端到端:进程重启后 wiki 页面不丢失
- [ ] 端到端:wiki 语义检索命中(向量召回)

---
