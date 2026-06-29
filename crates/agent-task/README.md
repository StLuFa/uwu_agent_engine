# agent-task

Agent **任务域** —— TaskManifest + SubtaskDAG 调度 + SettlementPolicy。

## 概述

Task 是跨多轮、可能跨多 Agent 的持久工作单元。SubtaskDAG 描述子任务间的依赖关系，SubtaskScheduler 按拓扑顺序调度。

```
Task → SubtaskDAG
         ├── Subtask A (Ready)      ──▶ 调度执行
         ├── Subtask B (Pending)    ──▶ 依赖 A 完成
         └── Subtask C (Ready)      ──▶ 并行执行
```

## 特性

- **Task + Goal** — 持久任务，含目标描述、成功标准、优先级
- **SubtaskDAG** — DAG 拓扑 + `ready_nodes()` 获取可执行节点
- **SubtaskScheduler** — 调度器：`next_ready()` + `is_complete()` + `progress()`
- **TaskManifest** — 任务清单：能力需求/预估 tokens/截止时间/优先级
- **AgentCard** — Agent 能力卡片（协作委派基础）
- **DelegationPolicy** — 委派策略：能力匹配/负载均衡/信任排序/竞标
- **SettlementPolicy** — 结算策略：免费/固定价/按量/竞标

## 安装

```toml
[dependencies]
agent-task = { path = "../agent-task" }
```

## 快速上手

### 创建任务

```rust
use agent_task::{Task, Goal, TaskManifest, TaskStatus};

let task = Task::new(
    Goal {
        description: "analyze dataset".into(),
        success_criteria: vec!["report generated".into()],
        priority: 5,
    },
    TaskManifest::new("Data Analysis", "analyze CSV and produce report")
        .with_capabilities(vec!["search".into(), "code".into()])
        .with_priority(5),
);
```

### SubtaskDAG 调度

```rust
use agent_task::subtask::{SubtaskDag, Subtask, SubtaskStatus};

let mut dag = SubtaskDag::new();
let idx_a = dag.add_node(Subtask::new(0, "fetch data"));
let idx_b = dag.add_node(Subtask::new(1, "analyze data"));
dag.add_edge(idx_a, idx_b);  // B depends on A

let ready = dag.ready_nodes();  // → only task A
```

### 进度追踪

```rust
use agent_task::scheduler::SubtaskScheduler;

let progress = SubtaskScheduler::progress(&dag);
let complete = SubtaskScheduler::is_complete(&dag);
let next = SubtaskScheduler::next_ready(&dag);
```

### 委派策略

```rust
use agent_task::delegation::{DelegationPolicy, DiscoveryStrategy, FallbackStrategy};

let policy = DelegationPolicy {
    discovery: DiscoveryStrategy::TrustRanked,
    fallback: FallbackStrategy::TryNext,
    max_retries: 3,
    timeout_secs: 300,
};
```

### 结算策略

```rust
use agent_task::settlement::{SettlementPolicy, SettlementMode};

let free = SettlementPolicy::free();
let paid = SettlementPolicy::fixed(1000, "payer", "payee");
```

## 核心类型

### Task

```rust
pub struct Task {
    pub task_id: TaskId,        // Uuid
    pub goal: Goal,             // { description, success_criteria, priority }
    pub status: TaskStatus,     // Created/Running/WaitingForDelegation/Completed/Failed/Cancelled
    pub subtask_dag: SubtaskDag,
    pub max_retries_per_subtask: u32,
    pub manifest: TaskManifest,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### SubtaskDag

```rust
pub struct SubtaskDag {
    pub nodes: Vec<Subtask>,
    pub edges: Vec<SubtaskEdge>,
}
```

方法：`add_node()`, `add_edge()`, `ready_nodes()`, `len()`

### TaskManifest

```rust
pub struct TaskManifest {
    pub title: String,
    pub description: String,
    pub required_capabilities: Vec<String>,
    pub estimated_tokens: u64,
    pub deadline_secs: Option<u64>,
    pub priority: u8,
}
```

## 目录结构

```
src/
├── lib.rs          // Task + Goal + TaskStatus + TaskId
├── subtask.rs      // Subtask + SubtaskDag + SubtaskStatus + SubtaskEdge + tests
├── manifest.rs     // TaskManifest + AgentCard
├── delegation.rs   // DelegationPolicy + DiscoveryStrategy + FallbackStrategy
├── settlement.rs   // SettlementPolicy + SettlementMode
└── scheduler.rs    // SubtaskScheduler
```

## 测试

```bash
cargo test -p agent-task
```

覆盖：DAG 单节点就绪、依赖阻塞、调度器进度计算。

## 依赖

- `agent-types-core` — AgentId
- `serde` + `chrono` + `uuid` — 序列化与标识

## License

与仓库一致。
