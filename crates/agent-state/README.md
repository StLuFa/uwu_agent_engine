# agent-state

Agent **状态维度** —— uwu_agent_engine 五维架构中最核心的维度。

## 概述

Agent 对"世界长什么样 + 任务进行到哪"的结构化理解。所有 Agent 决策基于 **AgentState** 而非 scratchpad 文本，是系统的 **唯一真相源（Single Source of Truth）**。

### 三层工作状态（WS）

| 层级 | 更新频率 | 版本号 | 内容 |
|---|---|---|---|
| `ShortTermWS` | 每步 +1 | `short_term.version` | 当前上下文、上一步动作、暂存假设 |
| `MidTermWS` | 每 N 步 +1 | `mid_term.version` | 动作历史、已知事实、交互模式、活跃约束 |
| `LongTermWS` | 任务级 +1 | `long_term.version` | 任务进度、累积预测误差、预算消耗 |

### MVCC 并发

- **主进程**：读写 State，写入时增加对应层的版本号
- **Sidecar**：通过 `snapshot()` 获取只读 `StateSnapshot`，不阻塞主进程
- **快照版本号** = `max(short.version, mid.version, long.version)`

### fork() 推演沙盒

```rust
let forked = state.fork();  // 完整 clone + 新 StateId + 链 parent_state_id
forked.apply_hypothetical(&action);  // 在沙盒中推演，不修改原 State
// 决策后丢弃 forked，原 state 毫发无损
```

## 特性

- **三层时间尺度状态** — 短/中/长程独立版本化，分别持久化和传输
- **MVCC 并发控制** — 主进程写入不阻塞 Sidecar 读取，基于版本号的快照隔离
- **fork() 推演沙盒** — 克隆状态 + 假设性执行，决策空间探索不污染主状态
- **JEPA 预测误差** — `compute_pred_error()` 计算预测与实际差异，`update_pred_error()` EMA 平滑累积（0.3×err + 0.7×accumulated）
- **StateDiff 结构化差异** — O(n+m) 按 key 比较已知事实，区分 added / modified / removed
- **StateScore 综合评分** — 三等权融合：事实一致性 + 目标对齐 + 约束满足
- **checkpoint / rollback** — serde_json 序列化检查点，崩溃后可恢复
- **ConfidenceMap** — 每项事实/假设的置信度追踪
- **InteractionPattern 检测** — `is_failure_loop()` / `is_loop_detected()` 供 Metacognition 消费
- **BudgetConsumed 预算追踪** — `cost_remaining_fraction()` 取 token/时间/重试最紧张维度

## 安装

```toml
[dependencies]
agent-state = { path = "../agent-state" }
```

## 快速上手

### 创建状态 + 应用动作

```rust
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};

let mut state = AgentState::new();

// 应用提交的动作
let action = Action::new("click", ActionParams::new().with("x", 100).with("y", 200));
state.apply_action(&action);

assert_eq!(state.short_term.version, 1);
assert!(state.short_term.last_action.is_some());
```

### fork 沙盒推演

```rust
let state = AgentState::new();
let mut sandbox = state.fork();

// 在沙盒中假设性执行
sandbox.apply_hypothetical(&action);

// 原状态不受影响
assert_ne!(sandbox.state_id, state.state_id);
assert_eq!(sandbox.parent_state_id.unwrap(), state.state_id);
```

### MVCC 快照

```rust
let mut state = AgentState::new();
state.short_term.version = 5;
state.mid_term.version = 3;
state.long_term.version = 7;

let snap = state.snapshot();
assert_eq!(snap.snapshot_version, 7);  // max(5, 3, 7)
```

### 状态差异 + 预测误差

```rust
use agent_state::mid::Fact;

let mut predicted = AgentState::new();
predicted.mid_term.known_facts.push(Fact::new("color", "red", 1.0));

let mut actual = AgentState::new();
actual.mid_term.known_facts.push(Fact::new("color", "blue", 1.0));

let diff = predicted.diff(&actual);
assert_eq!(diff.facts_modified.len(), 1);

// EMA 更新累积预测误差
predicted.update_pred_error(&actual);
// accumulated = 0.3 * 1.0 + 0.7 * 0.0 = 0.3
```

### 综合评分

```rust
let state = AgentState::new();
let score = state.evaluate();

println!("total:      {:.2}", score.total);       // 综合评分 [0,1]
println!("consistent: {:.2}", score.fact_consistency);  // 事实一致性
println!("aligned:    {:.2}", score.goal_alignment);     // 目标对齐
println!("constrained:{:.2}", score.constraint_satisfaction); // 约束满足
```

### checkpoint / rollback

```rust
let mut state = AgentState::new();
state.short_term.version = 42;

let checkpoint = state.checkpoint();
// ... 执行可能有副作用的操作 ...
let restored = AgentState::rollback(&checkpoint);

assert_eq!(restored.short_term.version, 42);
```

### InteractionPattern + 失败循环检测

```rust
use agent_state::mid::InteractionPattern;

let pattern = InteractionPattern {
    recent_success_rate: 0.2,
    detected_pattern: Some("loop_detected".into()),
    pattern_since_step: 5,
};

if pattern.is_failure_loop(0.3, 5) {
    // Metacognition → MetaAction::SwitchStrategy
}
```

### 预算追踪

```rust
use agent_state::long::BudgetConsumed;
use chrono::Duration;

let consumed = BudgetConsumed {
    tokens_used: 3000,
    elapsed: Duration::seconds(30),
    retries: 2,
};

let remaining = consumed.cost_remaining_fraction(
    10_000,                    // max_tokens
    Duration::seconds(120),    // max_time
    5,                         // max_retries
);
// remaining = min(0.7, 0.75, 0.6) = 0.6（重试维度最紧张）
```

## 核心类型

### AgentState

```rust
pub struct AgentState {
    pub state_id: StateId,
    pub timestamp: DateTime<Utc>,
    pub short_term: ShortTermWS,
    pub mid_term: MidTermWS,
    pub long_term: LongTermWS,
    pub confidence: ConfidenceMap,
    pub parent_state_id: Option<StateId>,  // fork 时设置
}
```

主要方法：

| 方法 | 说明 |
|---|---|
| `new()` | 创建空状态 |
| `fork()` | 克隆 + 新 ID + 链接父状态（不修改原状态） |
| `apply_action(&mut self, action)` | 提交动作：short_term.version += 1，记录 Committed |
| `apply_hypothetical(&mut self, action)` | 沙盒推演：记录 Hypothetical，不增版本号 |
| `snapshot()` | 生成 MVCC 只读 StateSnapshot |
| `diff(&self, other)` | 比较已知事实差异 → StateDiff |
| `compute_pred_error(&self, actual)` | JEPA 预测误差 [0,1] |
| `update_pred_error(&mut self, actual)` | EMA 平滑更新 accumulated_pred_error |
| `evaluate()` | 综合评分 → StateScore |
| `checkpoint()` | 序列化检查点 |
| `rollback(checkpoint)` | 从检查点恢复 |

### ShortTermWS（每步更新）

```rust
pub struct ShortTermWS {
    pub version: u64,
    pub current_context: ContextDescriptor,
    pub last_action: Option<Action>,
    pub last_observation: Option<String>,
    pub pending_hypotheses: Vec<Hypothesis>,
}
```

### MidTermWS（每 N 步更新）

```rust
pub struct MidTermWS {
    pub version: u64,
    pub action_history: Vec<ActionRecord>,
    pub known_facts: Vec<Fact>,
    pub recent_pattern: Option<InteractionPattern>,
    pub active_constraints: Vec<Constraint>,
}
```

### LongTermWS（任务级更新）

```rust
pub struct LongTermWS {
    pub version: u64,
    pub task_progress: TaskProgress,
    pub accumulated_pred_error: f32,   // EMA 平滑
    pub budget_consumed: BudgetConsumed,
}
```

### StateSnapshot（MVCC 快照）

```rust
pub struct StateSnapshot {
    pub snapshot_version: u64,  // max(三层 version)
    pub short_term: ShortTermWS,
    pub mid_term: MidTermWS,
    pub long_term: LongTermWS,
    pub taken_at: DateTime<Utc>,
}
```

## 数据流

```
                  ┌─────────────┐
用户输入 ──────▶  │  Perception  │
                  └──────┬──────┘
                         │ ContextDescriptor
                         ▼
┌──────────────────────────────────────────────────────┐
│  AgentState (唯一真相源)                               │
│                                                      │
│  ┌────────────┐  ┌────────────┐  ┌────────────────┐  │
│  │ ShortTermWS │  │  MidTermWS │  │  LongTermWS    │  │
│  │ 每步更新     │  │  每N步更新  │  │  任务级更新     │  │
│  │ v+=1        │  │  v+=1     │  │  v+=1          │  │
│  └─────┬──────┘  └─────┬──────┘  └───────┬────────┘  │
│        │               │                 │           │
│        └───────────────┼─────────────────┘           │
│                        │ MVCC versioning             │
└────────────────────────┼─────────────────────────────┘
                         │ snapshot()
                         ▼
              ┌─────────────────────┐
              │  StateSnapshot       │
              │  (Sidecar 只读消费)   │
              └─────────────────────┘

推演流程:
  state.fork() → sandbox.apply_hypothetical() → evaluate() → 决策 → 丢弃 sandbox
                                                                    或 commit
```

## 与其他维度的关系

```
agent-state  ─── 读 ───▶  agent-reaction       (ReactionRule::matches)
             ─── 读 ───▶  agent-reasoning       (推理基于当前 State)
             ─── 读 ───▶  agent-metacognition   (evaluate + InteractionPattern)
             ─── 读写 ──▶  agent-execution        (执行后 apply_action)
             ──▶ 写 ───▶  agent-learning        (update_pred_error)
             ─── 快照 ──▶ agent-session          (StateSnapshot → Sidecar)
```

## 目录结构

```
src/
├── lib.rs          # 模块声明 + re-exports
├── state.rs        # AgentState + StateId + 全部方法
├── short.rs        # ShortTermWS + ContextDescriptor + Hypothesis
├── mid.rs          # MidTermWS + ActionRecord + Fact + Constraint + InteractionPattern
├── long.rs         # LongTermWS + TaskProgress + BudgetConsumed
├── diff.rs         # StateDiff + compute_pred_error
├── evaluate.rs     # StateScore + evaluate
├── checkpoint.rs   # StateCheckpoint + checkpoint/rollback
├── mvcc.rs         # StateSnapshot + MVCC 版本号管理
└── confidence.rs   # ConfidenceMap
```

## 测试

```bash
cargo test -p agent-state
```

覆盖：fork 不修改原 State、apply_action 版本号递增、apply_hypothetical 不增版本、snapshot 版本号 = max(三层)、pred_error EMA 收敛、checkpoint → rollback 往返、StateDiff added/modified/removed/unmodified、InteractionPattern 失败循环检测、evaluate 各场景评分。

## 依赖

- `agent-types-core` — Action / ActionParams / ActionStatus / AgentId
- `serde` + `serde_json` — 序列化（StateCheckpoint 用 JSON）
- `chrono` — 时间戳与 Duration
- `uuid` — 全局唯一 ID 生成

## License

与仓库一致。
