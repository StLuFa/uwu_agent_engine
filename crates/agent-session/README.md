# agent-session

Agent **对话域** —— 持有五维 + 编排 `process_turn` 全流程主循环。

## 概述

Session 是 Agent 引擎的顶层入口：一个用户会话 = 一个 Session 实例。它将五维（Reaction/State/Metacognition/Persona/Character）组装成完整的决策循环。

```
每个 turn:
  1. Reaction.intercept(state)   ── Hit ──▶ 0 token 直接执行
  2. FlowGraph (P→M→R→E)        ──▶ 生成 Action
  3. Metacognition.evaluate()    ──▶ MetaAction 建议
  4. MetaAction 分支处理          ──▶ Proceed/Retry/Clarify/Switch/Abort
  5. Execution + Guard           ──▶ 执行 Action → 更新 State
  6. Metacognition.calibrate()   ──▶ EMA 更新 pred_error
```

## 特性

- **完整主循环** — 6 段式 `process_turn()`：Reaction → FlowGraph → Metacognition → MetaAction → Execution → Calibrate
- **MetaAction 6 分支** — Proceed / RetryDecision（回滚+重推理）/ RequestClarification / SwitchStrategy / DelegateToHuman / AbortOnBudget
- **IntentTracker** — 跨 turn 意图追踪 + 循环检测
- **ConversationHistory** — 对话历史 + token 统计 + Reaction 命中率
- **SessionSnapshot** — MVCC 快照供 Sidecar 消费
- **五维持有** — Reaction/State/Metacognition/Persona/Character 全部整合

## 安装

```toml
[dependencies]
agent-session = { path = "../agent-session" }
```

## 快速上手

### 创建 Session

```rust
use agent_session::Session;
use agent_state::AgentState;
use agent_reaction::ReactionLayer;
use agent_metacognition::Metacognition;
use agent_persona::{Persona, Identity, PersonaHistory, RelationshipGraph};
use agent_character::{Character, Preferences};
use agent_types_core::AgentId;
use chrono::Duration;

let session = Session {
    session_id: SessionId::new(),
    user_id: "user-1".into(),
    agent_id: AgentId::new(),
    reaction: Arc::new(ReactionLayer::builder().build()),
    state: AgentState::new(),
    metacognition: Metacognition::new(/* calibration_model */),
    persona: Persona { /* ... */ },
    character: Arc::new(Character { /* ... */ }),
    history: ConversationHistory::new(),
    intent_tracker: IntentTracker::new(),
    turn_count: 0,
    created_at: Utc::now(),
    last_active_at: Utc::now(),
};
```

### 处理对话

```rust
let mut session = /* ... */;
let result = session.process_turn("search for documents").await;
println!("output: {}", result.output.content);
println!("tokens: {}", result.output.tokens_used);
```

### 意图追踪

```rust
session.intent_tracker.update(Some("search".into()));
session.intent_tracker.update(Some("search".into()));

// 检测是否陷入循环
if session.intent_tracker.is_stuck(3) {
    // 切换策略
}
```

### 对话历史

```rust
session.history.push(turn);
println!("total tokens: {}", session.history.total_tokens());
println!("reaction hit rate: {:.1}%", session.history.reaction_hit_rate() * 100.0);

let last_3 = session.history.recent(3);
```

### Session 快照

```rust
let snap = session.snapshot();
// → SessionSnapshot { state_snapshot, persona_version, turn_count, total_tokens }
// Sidecar 消费
```

## 决策流程图

```
process_turn(input):
  │
  ├─ 1. enrich_input → EnrichedInput { persona_context, character_context }
  │
  ├─ 2. Reaction.intercept(state)
  │      ├─ Hit  → execute_reaction → record_turn → return (0 tokens)
  │      └─ Miss → continue
  │
  ├─ 3. FlowGraph (P→M→R→E) → Action
  │      └─ run_flowgraph(input) / run_flowgraph_degraded(input)
  │
  ├─ 4. Metacognition.evaluate(state, action)
  │      └─ MetacognitiveAssessment { meta_score, suggested_action }
  │
  ├─ 5. MetaAction 分支:
  │      Proceed              → 继续执行
  │      RetryDecision        → checkpoint.rollback() + 重推理
  │      RequestClarification → 暂停，向用户提问
  │      SwitchStrategy       → 降级推理模式
  │      DelegateToHuman      → 升级
  │      AbortOnBudget        → 终止
  │
  ├─ 6. execute_and_update(action) → apply_action → TurnResult
  │
  └─ 7. calibrate_with_outcome() → update_pred_error(EMA) + 追加校准记录
```

## 核心类型

### Session

```rust
pub struct Session {
    pub session_id: SessionId,
    pub user_id: String,
    pub agent_id: AgentId,
    pub reaction: Arc<ReactionLayer>,
    pub state: AgentState,
    pub metacognition: Metacognition,
    pub persona: Persona,
    pub character: Arc<Character>,
    pub history: ConversationHistory,
    pub intent_tracker: IntentTracker,
    pub turn_count: u64,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}
```

方法：`process_turn(input)`, `snapshot()`

### ConversationHistory

方法：`push(turn)`, `recent(n)`, `len()`, `total_tokens()`, `reaction_hit_rate()`, `iter()`

### IntentTracker

方法：`update(intent)`, `is_stuck(threshold)`, `infer(text)`

字段：`current_intent`, `previous_intent`, `intent_changed`, `consecutive_same_intent`, `total_turns`

### ConversationTurn

字段：`turn_number`, `user_input`, `agent_output`, `tokens_used`, `reaction_hit`, `meta_action`, `success`, `timestamp`

### SessionSnapshot

字段：`session_id`, `state_snapshot`, `persona_version`, `turn_count`, `total_tokens`, `taken_at`

## 目录结构

```
src/
├── lib.rs      // Session + process_turn + EnrichedInput + TurnResult + tests
├── turn.rs     // ConversationTurn
├── intent.rs   // IntentTracker + tests
├── history.rs  // ConversationHistory + tests
└── snapshot.rs // SessionSnapshot
```

## 测试

```bash
cargo test -p agent-session
```

覆盖：process_turn 完整流程执行、历史记录更新、意图追踪（变更检测/循环检测/推断）、快照生成、Reaction 命中率计算。

## 依赖

- `agent-state` + `agent-reaction` + `agent-metacognition` + `agent-persona` + `agent-character` — 五维
- `agent-types-core` — AgentId
- `serde` + `chrono` + `uuid` — 序列化与标识

## License

与仓库一致。
