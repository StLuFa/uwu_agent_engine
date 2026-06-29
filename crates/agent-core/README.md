# agent-core

Agent **编排核心** —— FlowGraph 管道配置 + FlowEngine 主循环执行器 + CapabilityRegistry 动态能力注册。

## 概述

agent-core 是 Agent 引擎的顶层编排层。它不实现新能力，而是将 Perception/Memory/Reasoning/Execution 各域组装成可执行的 `P→M→R→E` 决策管道。

```
FlowGraph                    FlowEngine
┌──────────┐                ┌──────────────────────────┐
│ P → M    │                │ for each stage:           │
│   ↓      │   ──run()──▶   │   Perception  → ctx      │
│   R → E  │                │   Memory      → ctx      │
│   ↓      │                │   Reasoning   → Decision │
│   V      │ (optional)     │   Execution   → Result   │
└──────────┘                │   Validate    → check    │
                            └──────────────────────────┘
```

## 特性

- **FlowGraph** — 声明式管道配置，standard() / high_security() / custom()
- **FlowEngine** — 按拓扑顺序执行管道阶段，传递 FlowContext
- **CapabilityRegistry** — 运行时动态注册各阶段的能力处理器（Perceiver/Reasoner/Executor）
- **热更新** — `add_edge_dynamic()` 运行时添加节点/边
- **FlowContext** — 阶段间共享上下文（原始输入 → context → memories → decision → output）

## 安装

```toml
[dependencies]
agent-core = { path = "../agent-core" }
```

## 快速上手

### 标准管道执行

```rust
use agent_core::{FlowGraph, FlowEngine, CapabilityRegistry};
use agent_state::AgentState;

let registry = CapabilityRegistry::new();
let engine = FlowEngine::new(registry);
let flow = FlowGraph::standard();     // P→M→R→E
let state = AgentState::new();

let ctx = engine.run(&flow, "user clicked submit", &state).await;

println!("context: {:?}", ctx.context_description);
println!("memories: {:?}", ctx.retrieved_memories);
println!("decision: {:?}", ctx.decision);
println!("execution: {:?}", ctx.execution_output);
```

### 高安全管道

```rust
let flow = FlowGraph::high_security(); // P→M→R→V→R→E (含验证回边)
let ctx = engine.run(&flow, "delete all records", &state).await;
// Validate 阶段会在执行前检查
```

### 注册能力处理器

```rust
use agent_core::{CapabilityRegistry, Stage};
use agent_core::capability::CapabilityHandler;

struct MyPerceiver;
impl CapabilityHandler for MyPerceiver {
    fn stage(&self) -> Stage { Stage::Perception }
    fn name(&self) -> &str { "my-perceiver" }
}

let mut registry = CapabilityRegistry::new();
registry.register(Box::new(MyPerceiver));
// 同一阶段可注册多个处理器（链式执行）
```

### 动态添加阶段

```rust
let mut flow = FlowGraph::standard();
flow.add_edge_dynamic(Stage::Execution, Stage::Perception); // 执行后重新感知
```

### 自定义管道

```rust
use agent_core::{FlowConfig, FlowGraph, Stage, FlowEdge};

let config = FlowConfig::custom(
    vec![Stage::Perception, Stage::Reasoning, Stage::Execution],
    vec![
        FlowEdge { from: Stage::Perception, to: Stage::Reasoning },
        FlowEdge { from: Stage::Reasoning, to: Stage::Execution },
    ],
);
let flow = FlowGraph::new(config);
```

## 核心类型

### FlowGraph

```rust
pub struct FlowGraph { pub config: FlowConfig }
```

方法：`new(config)`, `standard()`, `high_security()`, `add_edge_dynamic(from, to)`, `stage_count()`, `edge_count()`, `has_validation()`

### FlowConfig

```rust
pub struct FlowConfig {
    pub stages: Vec<Stage>,
    pub edges: Vec<FlowEdge>,
    pub validation_loop: bool,
}
```

方法：`standard()`, `high_security()`, `custom(stages, edges)`, `add_stage(stage, from)`, `predecessors(stage)`

### Stage 枚举

```rust
pub enum Stage { Perception, Memory, Reasoning, Execution, Validate }
```

### FlowEngine

```rust
pub struct FlowEngine { registry: CapabilityRegistry }
```

方法：`new(registry)`, `run(flow, input, state) -> FlowContext`, `registry()`

### FlowContext

```rust
pub struct FlowContext {
    pub raw_input: String,
    pub state: AgentState,
    pub context_description: Option<String>,
    pub retrieved_memories: Vec<String>,
    pub decision: Option<Decision>,
    pub execution_output: Option<String>,
    pub completed_stages: Vec<Stage>,
    pub log: Vec<String>,
}
```

### CapabilityRegistry

```rust
pub struct CapabilityRegistry { /* handlers by Stage */ }
```

方法：`register(handler)`, `get(stage)`, `count(stage)`, `stage_count()`, `total_count()`, `has_stage(stage)`, `stages()`

### CapabilityHandler trait

```rust
pub trait CapabilityHandler: Send + Sync {
    fn stage(&self) -> Stage;
    fn name(&self) -> &str;
}
```

## 标准管道拓扑

```
Perception ──▶ Memory ──▶ Reasoning ──▶ Execution

高安全模式:
  ⋯ Reasoning ──▶ Validate ──▶ Reasoning ──▶ Execution
```

## 目录结构

```
src/
├── lib.rs          // re-exports + 集成测试
├── flow.rs         // FlowGraph + FlowConfig + FlowEdge + Stage + tests
├── engine.rs       // FlowEngine + FlowContext + Decision + tests
└── capability.rs   // CapabilityRegistry + CapabilityHandler + tests
```

## 测试

```bash
cargo test -p agent-core
```

覆盖：standard/high_security 管道执行、FlowEngine 完整 P→M→R→E 流程、含注册处理器执行、FlowConfig predecessors/add_stage、dynamic edge、CapabilityRegistry 多处理器/空注册/按 stage 计数。

## 依赖

- `agent-state` — AgentState
- `agent-types-core` — Action/ActionParams
- `serde` + `serde_json` — 序列化
- `tokio` — async 运行时

## License

与仓库一致。
