# agent-guard

Agent **安全守卫** —— 五层硬闸门，编译期注册，运行时不可绕过。

## 概述

GuardLayer 位于 Execution 环节，在执行每个 Action 前通过五层闸门逐一检查。任何一层触发违规 → 拒绝执行 + 记录审计日志。

```
Action → enforce()
  ├─ 1. InstructionRule  → 指令检查（禁止 rm_rf / exec / shell）
  ├─ 2. ParameterRule    → 参数检查（文件大小 / 端口白名单）
  ├─ 3. CapabilityRule   → 能力检查（未注册能力拒绝）
  ├─ 4. BudgetRule       → 预算检查（token / 重试 耗尽）
  └─ 5. EgressRule       → 出站检查（MCP 写入白名单 / 内网禁止）
       ↓
  Allowed → 执行 | Blocked → AuditLog + Err(violations)
```

## 特性

- **五层独立闸门** — 每层独立 trait，各自注册，互不耦合
- **编译期注册** — `GuardBuilder` builder 模式，规则编译时绑定
- **不可自提升** — 规则列表构造后不可变，Agent 无法自行添加/移除规则
- **审计日志** — `AuditLog` 记录所有 Guard 命中事件
- **7 个内置规则** — 开箱即用的安全规则
- **Egress 独立检查** — 学习写入出站前额外校验

## 安装

```toml
[dependencies]
agent-guard = { path = "../agent-guard" }
```

## 快速上手

### 构建 GuardLayer

```rust
use agent_guard::{
    GuardLayer, AgentContext,
    rules::{NoRmRfRule, TokenBudgetRule, McpWriteAllowlistRule},
};
use agent_types_core::{Action, ActionParams};

let guard = GuardLayer::builder()
    .add_instruction_rule(NoRmRfRule)
    .add_parameter_rule(FileSizeLimitRule { max_bytes: 10 * 1024 * 1024 })
    .add_budget_rule(TokenBudgetRule)
    .add_egress_rule(McpWriteAllowlistRule {
        allowed_targets: vec!["safe-server".into()],
    })
    .build();
```

### 强制执行

```rust
let actions = vec![
    Action::new("click", ActionParams::new()),
    Action::new("rm_rf", ActionParams::new()), // blocked!
];

let ctx = AgentContext {
    session_id: "s1".into(),
    agent_id: "a1".into(),
    tokens_used: 500,
    max_tokens: 1000,
    retries: 0,
    max_retries: 5,
};

match guard.enforce(&actions, &ctx).await {
    Ok(allowed) => println!("{} actions allowed", allowed.len()),
    Err(violations) => {
        for v in &violations {
            eprintln!("BLOCKED [{}]: {}", v.rule, v.message);
        }
    }
}
```

### Egress 检查

```rust
match guard.check_egress("safe-server/api").await {
    Ok(()) => println!("egress allowed"),
    Err(v) => eprintln!("egress blocked: {}", v.message),
}
```

### 审计日志

```rust
let log = // ... from guard
println!("total events: {}", log.total_events());
println!("blocked: {}", log.blocked_count());
let recent = log.recent(10);
```

### 自定义规则

```rust
use agent_guard::{InstructionRule, GuardViolation, ViolationLevel};
use agent_types_core::Action;
use async_trait::async_trait;

struct NoGoogleRule;

#[async_trait]
impl InstructionRule for NoGoogleRule {
    async fn check(&self, action: &Action) -> Option<GuardViolation> {
        if action.command.contains("google") {
            return Some(GuardViolation {
                rule: "no-google".into(),
                level: ViolationLevel::Warning,
                message: "Google access denied".into(),
            });
        }
        None
    }
}
```

## 五层闸门

| 闸门 | Trait | 检查内容 |
|---|---|---|
| Instruction | `InstructionRule` | `action.command` 关键词 |
| Parameter | `ParameterRule` | `action.params` 值范围 |
| Capability | `CapabilityRule` | 能力注册状态 |
| Budget | `BudgetRule` | token/重试/时间预算 |
| Egress | `EgressRule` | 出站目标白名单/黑名单 |

## 内置规则

| 规则 | 层 | 说明 |
|---|---|---|
| `NoRmRfRule` | Instruction | 禁止 rm_rf / delete_all / drop_table / format |
| `NoShellExecutionRule` | Instruction | 禁止 exec / system / shell |
| `FileSizeLimitRule` | Parameter | 文件大小上限 |
| `PortAllowlistRule` | Parameter | 端口白名单 |
| `TokenBudgetRule` | Budget | Token 耗尽检查 |
| `RetryBudgetRule` | Budget | 重试次数检查 |
| `McpWriteAllowlistRule` | Egress | MCP 写入目标白名单 |
| `NoNetworkToInternalRule` | Egress | 禁止 10.x / 192.168.x / 172.16.x |

## 核心类型

### GuardLayer

```rust
pub struct GuardLayer { /* 5 层规则列表 + AuditLog */ }
```

方法：`builder()`, `enforce(actions, ctx)`, `check_egress(target)`

### GuardViolation

```rust
pub struct GuardViolation {
    pub rule: String,
    pub level: ViolationLevel,  // Warning / Critical
    pub message: String,
}
```

### AgentContext

```rust
pub struct AgentContext {
    pub session_id: String,
    pub agent_id: String,
    pub tokens_used: u64,
    pub max_tokens: u64,
    pub retries: u32,
    pub max_retries: u32,
}
```

## 目录结构

```
src/
├── lib.rs       // 5 trait + GuardLayer + GuardBuilder + enforce + tests
├── audit.rs     // AuditLog + AuditEvent + test
└── rules/
    └── mod.rs   // 8 个内置规则 + tests
```

## 测试

```bash
cargo test -p agent-guard
```

覆盖：enforce 放行/阻断/预算耗尽、check_egress 放行/阻断、Builder 全层注册、8 个内置规则各场景、审计日志记录。

## 依赖

- `agent-types-core` — Action/ActionParams
- `async-trait` — async trait
- `serde` + `chrono` — 序列化

## License

与仓库一致。
