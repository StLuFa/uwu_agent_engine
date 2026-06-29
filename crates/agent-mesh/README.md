# agent-mesh

Agent **语义事件网格** —— 对 `uwu_event_mesh` 的领域包装。

## 概述

不重复实现底层事件机制，仅定义 Agent 领域的：
- **topic 命名空间**（`agent.state.snapshot` / `agent.task.created` / ...）
- **事件类型**（StateSnapshotEvent / TaskCreated / DecisionMade / ...）
- **AgentTypeRegistry**（启动期一次性注册所有 Agent 事件类型）
- **AgentMesh 门面**（包装 EventMesh + FlowHandle）

### 四路通道语义

| 通道 | 容量 | 语义 |
|---|---|---|
| Main | 64 | 主循环（决策→执行） |
| Consolidation | 256 | Sidecar（LearnNode+Guard） |
| Monitoring | 64 | Sidecar（异常检测） |
| System | 128 | 心跳/配置/关闭 |

## 特性

- **9 种 Agent 事件类型** — 覆盖 State/Task/Decision/Persona 四个域
- **类型安全注册** — `AgentTypeRegistry::register_all()` 启动期一次性注册，跨进程反序列化校验
- **AgentMesh 门面** — 包装底层 EventMesh + FlowHandle，统一入口
- **Topic 命名约定** — 层级命名空间 `agent.<domain>.<event>`
- **Serde 序列化** — 所有事件支持 JSON 往返

## 安装

```toml
[dependencies]
agent-mesh = { path = "../agent-mesh" }
```

## 快速上手

### 创建 AgentMesh

```rust
use std::sync::Arc;
use agent_mesh::AgentMesh;
use uwu_event_mesh::EventMesh;

let event_mesh = Arc::new(EventMesh::new());
// FlowHandle 需配合 TypeRegistry 创建（见 uwu_event_mesh 文档）
// let agent_mesh = AgentMesh::new(event_mesh, flow_handle);
```

### 事件类型

```rust
use agent_mesh::events::{
    state::StateSnapshotEvent,
    task::{TaskCreated, TaskCompleted},
    decision::DecisionMade,
    persona::PersonaUpdated,
};

// State 快照
let snap = StateSnapshotEvent::new("agent-1", r#"{"version":1}"#, 1);

// 任务生命周期
let created = TaskCreated::new("task-1", "analyze data", 5, "agent-1");
let done = TaskCompleted::new("task-1", "agent-1", true, "analysis complete");

// 决策记录
let dec = DecisionMade::new("agent-1", "click submit", 0.85, "Proceed", 100);

// Persona 变更
let upd = PersonaUpdated::new("agent-1", 42, "collaboration completed");
```

### 序列化/反序列化

```rust
let event = TaskCreated::new("t1", "test", 3, "a1");
let json = serde_json::to_string(&event).unwrap();
let back: TaskCreated = serde_json::from_str(&json).unwrap();
assert_eq!(back.task_id, "t1");
```

### Topic 常量

```rust
use agent_mesh::topics::{
    TOPIC_STATE, TOPIC_TASK, TOPIC_DECISION, TOPIC_PERSONA,
    TOPIC_STATE_SNAPSHOT, TOPIC_TASK_CREATED, TOPIC_DECISION_MADE,
};

// 通配符订阅
// mesh.subscribe_str(TOPIC_TASK) → agent.task.>
// 精确 topic
// mesh.publish(TOPIC_TASK_CREATED, &event)
```

## 事件类型总览

### State 域 (`agent.state.>`)

| 事件 | Topic | 字段 |
|---|---|---|
| `StateSnapshotEvent` | `agent.state.snapshot` | event_id, agent_id, snapshot_json, snapshot_version, timestamp |

### Task 域 (`agent.task.>`)

| 事件 | Topic | 字段 |
|---|---|---|
| `TaskCreated` | `agent.task.created` | task_id, goal_description, priority, created_by |
| `TaskCompleted` | `agent.task.completed` | task_id, completed_by, success, summary |
| `SubtaskDelegated` | `agent.task.subtask_delegated` | task_id, subtask_id, delegated_by, delegated_to, description |
| `DelegationResult` | `agent.task.delegation_result` | task_id, subtask_id, completed_by, success, result_summary |

### Decision 域 (`agent.decision.>`)

| 事件 | Topic | 字段 |
|---|---|---|
| `DecisionMade` | `agent.decision.made` | agent_id, decision_text, meta_score, meta_action, tokens_used |
| `DecisionRetried` | `agent.decision.retried` | agent_id, original_decision_id, reason, retry_count |

### Persona 域 (`agent.persona.>`)

| 事件 | Topic | 字段 |
|---|---|---|
| `PersonaUpdated` | `agent.persona.updated` | agent_id, new_version, change_description |
| `RelationshipChanged` | `agent.persona.relationship_changed` | agent_id, peer_id, new_trust, trust_delta |

## 核心类型

### AgentMesh

```rust
pub struct AgentMesh {
    pub mesh: Arc<EventMesh>,
    pub flow: FlowHandle,
    pub type_registry: Arc<TypeRegistry>,
}
```

构造时自动调用 `AgentTypeRegistry::register_all()`。

### AgentTypeRegistry

```rust
pub struct AgentTypeRegistry;

impl AgentTypeRegistry {
    pub fn register_all(registry: &Arc<TypeRegistry>);
}
```

注册全部 9 种事件类型，TypeId 格式为 `agent_<domain>.<event>`（底层 TypeId 不允许 domain 含点）。

## 目录结构

```
src/
├── lib.rs          // AgentMesh 门面 + re-exports + tests
├── topics.rs       // Topic 命名空间常量（4 通配符 + 8 精确）
├── registry.rs     // AgentTypeRegistry::register_all()
└── events/
    ├── mod.rs
    ├── state.rs    // StateSnapshotEvent
    ├── task.rs     // TaskCreated / TaskCompleted / SubtaskDelegated / DelegationResult
    ├── decision.rs // DecisionMade / DecisionRetried
    └── persona.rs  // PersonaUpdated / RelationshipChanged
```

## 测试

```bash
cargo test -p agent-mesh
```

覆盖：9 种事件 serde 往返、topic 常量验证、TypeRegistry 注册全部类型。

## 依赖

- `uwu_event_mesh` — 底层事件网格（EventMesh / FlowHandle / TypeRegistry）
- `agent-types-core` — 基础类型
- `serde` + `serde_json` + `chrono` + `uuid` — 序列化与标识

## License

与仓库一致。
