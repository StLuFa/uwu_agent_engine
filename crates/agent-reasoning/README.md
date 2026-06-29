# agent-reasoning

Agent **推理域** —— 消费 State + fork() 推演沙盒 + Tree-of-Thought beam search。

## 概述

Reasoning 是 Agent 决策循环的核心：消费当前 `AgentState`，通过沙盒推演评估候选动作，输出 `Decision`。

```
State + Goal → [生成候选] → fork() 沙盒推演 → evaluate() 评分 → 选最优 → Decision
                                  │
                    根据 TTS 信号切换策略:
                      Normal   → ToT beam search
                      Degraded → 单步推理（禁用 ToT）
                      Urgent   → 直接回答（禁止新工具）
                      Abort    → 终止
```

作为 visual_script NodeDefinition 注册：`"reasoning.decide"`（Impure + Async）。

## 特性

- **Reasoner trait** — 异步推理器接口，消费 State + Goal → Decision
- **SandboxEvaluator** — fork State → apply_hypothetical → evaluate → 收集评分
- **ToTExplorer** — Tree-of-Thought beam search（Beam width + 深度限制 + 最低分剪枝）
- **4 级策略切换** — Normal/Degraded/Urgent/Abort，根据 TTS 信号自动选择
- **候选排序** — 按 State 评分降序，best_action() 返回最优

## 安装

```toml
[dependencies]
agent-reasoning = { path = "../agent-reasoning" }
```

## 快速上手

### 单步推理

```rust
use agent_reasoning::{Decision, SandboxEvaluator};
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};

let state = AgentState::new();
let candidates = vec![
    Action::new("search", ActionParams::new().with("query", "rust")),
    Action::new("click", ActionParams::new().with("target", "btn")),
];

// 沙盒推演评分
let results = SandboxEvaluator::evaluate_candidates(&state, &candidates);
for (action, score) in &results {
    println!("{}: {:.2}", action.command, score);
}

// 选最优
let best = SandboxEvaluator::best_candidate(&state, &candidates);
```

### ToT beam search

```rust
use agent_reasoning::{ToTExplorer, ToTConfig};
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};

let state = AgentState::new();
let config = ToTConfig {
    beam_width: 3,           // 每层保留 3 个候选
    max_depth: 4,            // 最多探索 4 层
    candidates_per_step: 5,  // 每步生成 5 个候选
    min_score: 0.3,          // 低于 0.3 剪枝
};

let explorer = ToTExplorer::new(config);

// 候选生成器（真实系统调用 LLM）
let result = explorer.explore(&state, "find user data", |s, depth| {
    vec![
        Action::new(format!("action_{depth}_a"), ActionParams::new()),
        Action::new(format!("action_{depth}_b"), ActionParams::new()),
    ]
});
```

### 策略切换

```rust
use agent_reasoning::ReasoningStrategy;

let strategy = ReasoningStrategy::from_cost_remaining(0.3);
assert!(!strategy.allows_tot());          // Degraded → 禁用 ToT
assert!(strategy.allows_new_tools());     // 仍可调用工具

let urgent = ReasoningStrategy::from_cost_remaining(0.1);
assert!(!urgent.allows_new_tools());      // Urgent → 禁止新工具

let abort = ReasoningStrategy::from_cost_remaining(0.0);
assert!(abort.should_abort());            // 终止
```

### 实现 Reasoner

```rust
use agent_reasoning::{Reasoner, Decision};
use agent_state::AgentState;
use async_trait::async_trait;

struct MyReasoner;

#[async_trait]
impl Reasoner for MyReasoner {
    async fn reason(&self, state: &AgentState, goal: &str, context: Option<&str>) -> Decision {
        // 调用 LLM 生成候选 → 沙盒推演 → 返回 Decision
        Decision::single(
            agent_types_core::Action::new("search", agent_types_core::ActionParams::new()),
            0.85,
            format!("reasoning for: {goal}"),
        )
    }
}
```

## 核心类型

### Decision

```rust
pub struct Decision {
    pub actions: Vec<Action>,    // 候选动作（按评分降序）
    pub scores: Vec<f32>,        // 每个候选的评分
    pub reasoning: String,      // 推理文本
}
```

方法：`new()`, `single()`, `best_action()`, `best_score()`

### Reasoner trait

```rust
#[async_trait]
pub trait Reasoner: Send + Sync {
    async fn reason(&self, state: &AgentState, goal: &str, context: Option<&str>) -> Decision;
}
```

### SandboxEvaluator

```rust
pub struct SandboxEvaluator;
```

方法：`evaluate_candidates(state, candidates) -> Vec<(Action, f32)>`, `best_candidate(state, candidates) -> Option<(Action, f32)>`

### ToTExplorer

```rust
pub struct ToTExplorer { config: ToTConfig }
```

方法：`new(config)`, `explore(state, goal, generate_candidates) -> Vec<Action>`

### ToTConfig

```rust
pub struct ToTConfig {
    pub beam_width: usize,          // 每层保留候选数（默认 3）
    pub max_depth: usize,           // 最大深度（默认 4）
    pub candidates_per_step: usize, // 每步生成数（默认 5）
    pub min_score: f32,             // 剪枝阈值（默认 0.3）
}
```

### ReasoningStrategy

```rust
pub enum ReasoningStrategy { Normal, Degraded, Urgent, Abort }
```

方法：`from_cost_remaining(cost)`, `allows_tot()`, `allows_new_tools()`, `should_abort()`

## TTS → 策略映射

| cost_remaining | 策略 | ToT | 工具调用 | 说明 |
|---|---|---|---|---|
| > 0.5 | Normal | ✅ | ✅ | ToT beam search |
| 0.2 ~ 0.5 | Degraded | ❌ | ✅ | 单步推理 |
| 0.05 ~ 0.2 | Urgent | ❌ | ❌ | 直接回答 |
| ≤ 0.05 | Abort | ❌ | ❌ | 终止 |

## 目录结构

```
src/
├── lib.rs          // ReasoningInput/Output + tests
├── reasoner.rs     // Decision + Reasoner trait
├── sandbox.rs      // SandboxEvaluator + tests
├── strategies.rs   // ReasoningStrategy + tests
└── tot.rs          // ToTExplorer + ToTConfig + tests
```

## 测试

```bash
cargo test -p agent-reasoning
```

覆盖：Decision 构造、SandboxEvaluator 多候选评分、最佳候选选取、ToT beam search 产出动作、ToTConfig 默认值、4 级策略分档边界。

## 依赖

- `agent-state` — AgentState（fork/evaluate/snapshot）
- `agent-types-core` — Action/ActionParams
- `async-trait` — async trait
- `serde` + `serde_json` — 序列化

## License

与仓库一致。
