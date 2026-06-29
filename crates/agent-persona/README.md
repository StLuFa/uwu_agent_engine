# agent-persona

Agent **人物角色维度（MVCC 版本化）** —— "我是谁"（会变）。

## 概述

Persona 是 Agent 的身份锚点，随经历增长而变化。与 Character 的区别：

| | Persona | Character |
|---|---|---|
| 可变性 | **可变**（每次协作更新） | **不可变**（核心价值观） |
| 内容 | 身份/关系网络/履历 | 核心价值观/决策偏好 |
| 版本 | MVCC 版本号 | 无版本号 |
| 更新 | `update_relationship()` → version += 1 | 构造后不提供 setter |

### MVCC 并发

- **主进程写入**：`update_relationship()` → version += 1
- **Sidecar 读取**：`snapshot()` → 只读 `PersonaSnapshot`

## 特性

- **身份管理** — `Identity` 含名称/角色/组织/背景/专长
- **关系图** — `RelationshipGraph` 维护 AgentId → 信任度/关系类型/协作次数的有向映射
- **信任计算** — `adjust_trust()` delta 调节 + `trusted_peers()` 阈值过滤
- **履历日志** — `PersonaHistory` 记录关键经历事件，支持按类型筛选
- **MVCC 快照** — `snapshot()` 生成只读快照供 Sidecar 消费
- **上下文注入** — `to_context_injection()` 生成可注入推理 prompt 的精简表示

## 安装

```toml
[dependencies]
agent-persona = { path = "../agent-persona" }
```

## 快速上手

### 创建 Persona

```rust
use agent_persona::{Persona, Identity};
use agent_persona::relationships::{RelationshipGraph, Relationship, RelationType};
use agent_types_core::AgentId;

let persona = Persona {
    version: 0,
    identity: Identity::new("Alice", "researcher")
        .with_organization("ACME Corp")
        .with_expertise(vec!["Rust".into(), "ML".into()]),
    relationships: RelationshipGraph::new(),
    history: PersonaHistory::new(),
};
```

### 更新关系

```rust
let mut persona = Persona { /* ... */ };
let peer = AgentId::new();

// 协作成功 → 增加信任
persona.update_relationship(peer.clone(), 0.2);
assert_eq!(persona.version, 1);

// 查看受信任的 peers
let trusted = persona.relationships.trusted_peers();
```

### 注入推理上下文

```rust
let ctx = persona.to_context_injection();
// → PersonaContext { name: "Alice", role: "researcher", ... }
// 注入到 LLM system prompt 中
```

### MVCC 快照

```rust
let snap = persona.snapshot();
// → PersonaSnapshot { version: 1, identity: ..., relationship_count: 1 }
// Sidecar 进程可安全读取，不阻塞主进程
```

### 关系图操作

```rust
let mut graph = RelationshipGraph::new();

// 添加关系
graph.upsert(
    AgentId::new(),
    Relationship::new(RelationType::Peer, 0.8),
);

// 查询信任度
let trust = graph.trust_for(&agent_id);

// 获取高信任 peers
let trusted = graph.trusted_peers(); // trust > 0.5，按信任度降序
```

### 履历记录

```rust
use agent_persona::history::{PersonaHistory, PersonaEvent};

let mut history = PersonaHistory::new();
history.push(PersonaEvent::new(
    "completed task #42 with agent B",
    "collaboration",
).with_agents(vec!["agent-b".into()]));

// 按类型筛选协作事件
let collaborations = history.by_type("collaboration");
// 最近 5 条
let recent = history.recent(5);
```

## 核心类型

### Persona

```rust
pub struct Persona {
    pub version: u64,           // MVCC 版本号
    pub identity: Identity,
    pub relationships: RelationshipGraph,
    pub history: PersonaHistory,
}
```

方法：`to_context_injection()`, `update_relationship(peer, trust_delta)`, `snapshot()`

### Identity

```rust
pub struct Identity {
    pub name: String,
    pub role: String,
    pub organization: String,
    pub background: String,
    pub expertise: Vec<String>,
}
```

### RelationshipGraph

```rust
pub struct RelationshipGraph { /* HashMap<AgentId, Relationship> */ }
```

方法：`upsert()`, `adjust_trust(agent, delta)`, `trust_for(agent)`, `trusted_peers()`, `len()`, `iter()`

### Relationship

```rust
pub struct Relationship {
    pub relation_type: RelationType,  // Peer / Supervisor / Subordinate / External
    pub trust: f32,                    // [0.0, 1.0]
    pub collaboration_count: u32,
    pub last_interaction: Option<String>,
}
```

### PersonaSnapshot

```rust
pub struct PersonaSnapshot {
    pub version: u64,
    pub identity: Identity,
    pub relationship_count: usize,
}
```

## 三层约束体系

```text
Character.core_values（HardConstraint） → 决策层约束（不可绕过）
Persona.relationships                   → 社交层约束（信任/协作策略）
GuardLayer（硬闸门）                     → 执行层约束（运行时不可绕过）
```

## 目录结构

```
src/
├── lib.rs             // Persona + PersonaContext + PersonaSnapshot + tests
├── identity.rs        // Identity
├── relationships.rs   // RelationshipGraph + Relationship + RelationType
└── history.rs         // PersonaHistory + PersonaEvent
```

## 测试

```bash
cargo test -p agent-persona
```

覆盖：版本号递增、trust 调整、snapshot、context_injection、trusted_peers 阈值过滤。

## 依赖

- `agent-types-core` — AgentId
- `serde` + `chrono` — 序列化与时间戳
- `uuid` — ID 生成

## License

与仓库一致。
