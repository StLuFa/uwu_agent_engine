# agent-reaction

Agent **反射短路维度** —— 每步决策前的第一道拦截闸门。

## 概述

Reaction 层在每步 LLM 调用前执行规则拦截，命中则**短路跳过 FlowGraph → LLM 的昂贵路径**，节省 30-50% token。

高频低智操作（弹窗关闭、限流重试、验证码检测、长期无进展）在此被确定性规则拦截，不进入推理循环。

```
每步决策:
  1. Reaction.intercept(state) ── Hit ──▶ 直接执行动作（0 token）
  2. FlowGraph (Perception → Memory → Reasoning → Execution)
  3. Metacognition 评估
  ...
```

## 特性

- **规则短路** — 顺序匹配规则，首个命中即返回，后续规则不执行
- **Builder 模式** — `ReactionLayer::builder().add_rule(r1).add_rule(r2).build()`
- **无锁统计** — `ReactionStats` 使用 `AtomicU64` 追踪 hits/misses，高频调用零阻塞
- **文本匹配规则** — 基于 `AgentState` 的 `description`/`last_observation` 做关键词匹配，确定性强
- **Async trait** — `ReactionRule::react()` 为 async，支持网络调用等异步动作
- **4 个内置规则** — 开箱即用，覆盖常见高频场景

## 安装

```toml
[dependencies]
agent-reaction = { path = "../agent-reaction" }
```

## 快速上手

### 基础用法

```rust
use agent_reaction::{Reaction, ReactionLayer};
use agent_reaction::rules::{PopupCloseRule, RateLimitRetryRule};
use agent_state::AgentState;

let state = AgentState::new();

// Builder 模式构建反应层
let layer = ReactionLayer::builder()
    .add_rule(PopupCloseRule)
    .add_rule(RateLimitRetryRule)
    .build();

match layer.intercept(&state).await {
    Reaction::Hit(action) => {
        println!("拦截命中: {}", action.command);
        // 直接执行 action，跳过 LLM
    }
    Reaction::Miss => {
        println!("未命中，进入 FlowGraph → LLM");
    }
}
```

### 自定义规则

```rust
use agent_reaction::ReactionRule;
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

struct GreetingRule;

#[async_trait]
impl ReactionRule for GreetingRule {
    fn matches(&self, state: &AgentState) -> bool {
        state.short_term.current_context
            .description
            .to_lowercase()
            .contains("hello")
    }

    async fn react(&self, _state: &AgentState) -> Action {
        Action::new("reply", ActionParams::new().with("text", "Hi there!"))
    }
}
```

### 查看统计

```rust
let layer = ReactionLayer::builder()
    .add_rule(PopupCloseRule)
    .build();

// ... 多次 intercept() 之后 ...

let stats = layer.stats();
println!("hits:      {}", stats.hits.load(Ordering::Relaxed));
println!("misses:    {}", stats.misses.load(Ordering::Relaxed));
println!("total:     {}", stats.total());
println!("hit_rate:  {:.2}%", stats.hit_rate() * 100.0);
```

### 与 Session 集成

```rust
// agent-session 中的使用方式
pub async fn process_turn(&mut self, raw_input: &str) -> TurnResult {
    // 1. Reaction 拦截（第一步）
    if let Reaction::Hit(action) = self.reaction.intercept(&self.state).await {
        return self.execute_reaction(action).await;  // 0 token
    }

    // 2. FlowGraph → LLM（昂贵路径）
    let decision = self.run_flowgraph(&input).await;
    // ...
}
```

## 内置规则

### PopupCloseRule (P1)

检测弹窗关键词，自动点击关闭按钮。

**触发条件**：`ContextDescriptor.description` 包含以下任一关键词（大小写不敏感）：
`popup`, `modal`, `dialog`, `close`, `弹窗`, `广告`, `×`, `newsletter`, `subscribe` 等

**响应**：`Action("click", {"target": "popup-close-button"})`

### RateLimitRetryRule (P1)

检测限流信号，自动等待后重试。

**触发条件**：`last_observation` 或 `description` 包含：
`429`, `rate limit`, `too many requests`, `retry after`, `限流` 等

**响应**：`Action("wait_retry", {"delay_ms": 5000})`

### CaptchaDetectRule (P2)

检测验证码，请求人工介入。

**触发条件**：`description` 包含：
`captcha`, `recaptcha`, `验证码`, `verify you are human` 等

**响应**：`Action("request_human", {"reason": "captcha"})`

### IdleTimeoutRule (P2)

检测长期无进展，重新评估目标。

**触发条件**（满足任一）：
- `mid_term.recent_pattern` 检测到失败循环（成功率 < 0.3 连续 ≥ 5 步）
- `short_term.last_action` 为空但有历史动作（停滞状态）

**响应**：`Action("re_evaluate_goal", {})`

## 核心类型

### Reaction 枚举

```rust
pub enum Reaction {
    Hit(Action),  // 规则命中，直接返回动作
    Miss,         // 未命中，进入 FlowGraph
}
```

### ReactionRule trait

```rust
#[async_trait]
pub trait ReactionRule: Send + Sync {
    fn matches(&self, state: &AgentState) -> bool;
    async fn react(&self, state: &AgentState) -> Action;
}
```

### ReactionLayer

```rust
pub struct ReactionLayer {
    rules: Vec<Box<dyn ReactionRule + Send + Sync>>,
    stats: ReactionStats,
}
```

方法：`builder()`, `intercept(&self, state) -> Reaction`, `stats() -> &ReactionStats`

### ReactionStats

```rust
pub struct ReactionStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
}
```

方法：`new()`, `total() -> u64`, `hit_rate() -> f32`

## 目录结构

```
src/
├── lib.rs              // ReactionLayer + ReactionRule trait + Builder + Reaction 枚举
├── stats.rs            // ReactionStats (AtomicU64 hits/misses)
└── rules/
    ├── mod.rs          // 子模块声明 + re-exports
    ├── popup_close.rs  // PopupCloseRule
    ├── rate_limit.rs   // RateLimitRetryRule
    ├── captcha.rs      // CaptchaDetectRule
    └── idle.rs         // IdleTimeoutRule
```

## 测试

```bash
cargo test -p agent-reaction
```

覆盖：4 个内置规则各 3-4 个 match/miss 场景、Hit 短路验证、Miss 穿透、统计计数正确性、hit_rate 计算、Builder 规则顺序。

## 与其他维度的关系

```
agent-reaction ── 读 ──▶ agent-state        (matches() 检查 AgentState)
               ── 返回 ──▶ agent-session     (intercept() → Hit/Miss 分支)
               ── 被绕过 ──▶ agent-reasoning  (Miss 时进入 FlowGraph)
               ── 被绕过 ──▶ agent-metacognition
```

在 Metacognition 的 TTS 预算分级中，当 `cost_remaining < 0.2` 进入 `Urgent` 模式时，仅允许 reaction 短路 + 直接回答。

## 依赖

- `agent-state` — 读取 AgentState 以判断规则匹配
- `agent-types-core` — Action 类型
- `async-trait` — async trait 支持
- `serde` — ReactionStats 序列化
- `tokio` — async 运行时

## License

与仓库一致。
