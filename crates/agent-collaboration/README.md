# agent-collaboration

Agent **多 Agent 协作** —— 委派 / 协商 + AgentRegistry。

## 概述

Collaboration 使多个 Agent 能够发现彼此、委派任务、协商协作。AgentRegistry 维护已知 Agent 的能力索引和信任评分。

```
Agent A                                Agent B
   │                                      │
   ├─ 1. registry.best_for_capability()   │
   ├─ 2. delegate(task, capability) ──────▶
   │                                      ├─ 3. 执行 subtask
   │   ◀────────── result ────────────────┤
   ├─ 4. on_delegation_complete()         │
   ├─ 5. 更新 state_delta + trust         │
```

## 特性

- **AgentRegistry** — Agent 注册/注销 + 能力索引 + 信任度排序
- **Capability Match** — `find_by_capability()` / `best_for_capability()`
- **Task Delegation** — `delegate()` 选择最优 Agent + `on_delegation_complete()` 处理结果
- **Delegation Lifecycle** — Pending → Accepted → Running → Completed/Failed/TimedOut
- **Negotiation** — `NegotiationResult`: accepted / rejected / counter_offer
- **Collaboration Facade** — 封装 registry + pending delegations
- **SharedState CRDT** — CRDT-based 共享状态同步
- **Wiki Delegation** — 通过 agent-wiki 进行知识委派

## 安装

```toml
[dependencies]
agent-collaboration = { path = "../agent-collaboration" }
```

## 快速上手

### 注册和发现 Agent

```rust
use agent_collaboration::{AgentRegistry, AgentDescriptor};
use agent_types_core::AgentId;

let mut registry = AgentRegistry::new();

// 注册 Agent
let desc = AgentDescriptor::new(AgentId::new(), "search-agent", "worker")
    .with_capabilities(vec!["search".into(), "click".into()])
    .with_trust(0.9);
registry.register(desc);

// 按能力发现
let searchers = registry.find_by_capability("search");
let best = registry.best_for_capability("search").unwrap();
```

### 委派任务

```rust
use agent_collaboration::Collaboration;
use std::sync::Arc;

let registry = Arc::new(registry);
let mut collab = Collaboration::new(registry);

// 委派
let result = collab
    .delegate("task-1", AgentId::new(), "search")
    .unwrap();

// 完成
let completed = collab
    .on_delegation_complete(&result.delegation_id, "found 5 results")
    .unwrap();

assert!(completed.is_done());
```

### 协商

```rust
use agent_collaboration::NegotiationResult;

let accepted = NegotiationResult::accepted(Some(500));
let rejected = NegotiationResult::rejected("insufficient capability");
let counter = NegotiationResult::counter_offer(300);
```

## 核心类型

### Collaboration

```rust
pub struct Collaboration {
    pub registry: Arc<AgentRegistry>,
    pub pending_delegations: Vec<DelegationResult>,
}
```

方法：`delegate(task_id, from, capability)`, `on_delegation_complete(id, output)`, `pending_count()`

### AgentRegistry

```rust
pub struct AgentRegistry { /* HashMap<AgentId, AgentDescriptor> */ }
```

方法：`register()`, `unregister()`, `get()`, `find_by_capability()`, `best_for_capability()`, `len()`

### AgentDescriptor

```rust
pub struct AgentDescriptor {
    pub agent_id: AgentId,
    pub name: String,
    pub capabilities: Vec<String>,
    pub role: String,
    pub trust_score: f32,
    pub is_available: bool,
    pub task_count: u64,
}
```

### DelegationResult

```rust
pub struct DelegationResult {
    pub delegation_id: DelegationId,
    pub task_id: String,
    pub from: AgentId,
    pub to: AgentId,
    pub state: DelegationState,
    pub output: Option<String>,
    pub tokens_used: u64,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
```

方法：`new()`, `complete(output)`, `fail(reason)`, `is_done()`

### DelegationState

```rust
pub enum DelegationState {
    Pending, Accepted, Running, Completed,
    Failed { reason: String }, TimedOut,
}
```

## 目录结构

```
src/
├── lib.rs        // Collaboration + tests
├── registry.rs   // AgentRegistry + AgentDescriptor + tests
├── delegate.rs   // DelegationId + DelegationState + DelegationResult + tests
└── negotiate.rs  // NegotiationResult
```

## 测试

```bash
cargo test -p agent-collaboration
```

覆盖：Agent 注册+能力发现、信任度排序选择、委派→完成生命周期、完成状态检查。

## 依赖

- `agent-types-core` — AgentId
- `agent-crdt` — SharedState CRDT 同步
- `agent-wiki` — Wiki 委派
- `serde` + `chrono` + `uuid` — 序列化与标识
- `async-trait` + `tokio` — async 支持

## License

与仓库一致。
