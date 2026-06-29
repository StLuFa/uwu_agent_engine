# agent-character

Agent **人格维度** —— 底层不可变的核心价值观（安全锚点）+ 上层可调整的决策偏好。

## 概述

Character 是 Agent 的"性格"——底层是**不可变**的核心价值观（硬/软约束），上层是**可调整**的决策偏好。

### 两层结构

| 层 | 可变性 | 内容 | 示例 |
|---|---|---|---|
| 核心价值观 | **不可变** | 硬/软约束 | "不泄露隐私"、"不执行破坏性命令" |
| 决策偏好 | 可调整 | 工具偏好/风险容忍度/策略 | "优先搜索"、"步骤式输出" |

### 三层约束体系

```text
Character.core_values（HardConstraint） → 决策层约束（不可绕过）
Persona.relationships                   → 社交层约束（信任/协作策略）
GuardLayer（硬闸门）                     → 执行层约束（运行时不可绕过）
```

## 特性

- **HardConstraint 硬约束** — 违反则阻断执行，返回 `Err(ValueViolation)`
- **SoftGuideline 软指导** — 警告但不阻断
- **关键词违反检测** — `CoreValue.violates()` 基于 `action.command` 关键词匹配
- **3 个内置预设** — `privacy_first()`, `honesty_first()`, `no_destructive_actions()`
- **可调偏好** — 风险容忍度、不确定策略、输出风格、工具偏好
- **上下文注入** — `to_context_injection()` 生成偏好注入字符串

## 安装

```toml
[dependencies]
agent-character = { path = "../agent-character" }
```

## 快速上手

### 创建 Character

```rust
use agent_character::{
    Character, Preferences, OutputStyle, UncertaintyStrategy,
    values::{CoreValue, ValueEnforcement},
};

let character = Character {
    core_values: vec![
        CoreValue::privacy_first(),         // 硬约束
        CoreValue::no_destructive_actions(), // 硬约束
        CoreValue::honesty_first(),          // 硬约束
        CoreValue::new(
            "prefer-concise",
            "prefer short responses unless asked",
            ValueEnforcement::SoftGuideline,
        ),
    ],
    preferences: Preferences::new()
        .with_risk_tolerance(0.3)
        .with_output_style(OutputStyle::StepByStep)
        .with_uncertainty_strategy(UncertaintyStrategy::SearchFirst),
};
```

### 检查价值观

```rust
use agent_types_core::{Action, ActionParams};

let safe_action = Action::new("click", ActionParams::new());
assert!(character.check_core_values(&safe_action).is_ok());

let bad_action = Action::new("delete_all_records", ActionParams::new());
match character.check_core_values(&bad_action) {
    Err(violation) => {
        eprintln!("Blocked: {} — {}", violation.value, violation.reason);
        // → "Blocked: no-destructive — 不执行破坏性命令"
    }
    Ok(()) => {}
}
```

### 注入推理上下文

```rust
let ctx = character.to_context_injection();
// → CharacterContext {
//     output_style: StepByStep,
//     uncertainty_strategy: SearchFirst,
//     risk_tolerance: 0.3,
// }
// 注入到 LLM system prompt
```

### 自定义价值观

```rust
let custom = CoreValue::new(
    "no-financial-advice",
    "不提供具体投资建议",
    ValueEnforcement::HardConstraint,
).with_forbidden(vec!["invest".into(), "buy_stock".into(), "trade".into()]);

// 违反检测基于 action.command 中的关键词
let action = Action::new("buy_stock", ActionParams::new().with("symbol", "AAPL"));
assert!(custom.violates(&action));
```

### 调整偏好

```rust
use agent_character::preferences::Preferences;

let prefs = Preferences::new()
    .with_risk_tolerance(0.8)                              // 激进
    .with_output_style(OutputStyle::Detailed)              // 详细输出
    .with_uncertainty_strategy(UncertaintyStrategy::BestGuessAndConfirm)
    .with_tools(vec!["web_search".into(), "calculator".into()]);
```

## 核心类型

### Character

```rust
pub struct Character {
    pub core_values: Vec<CoreValue>,  // 构造后不可变
    pub preferences: Preferences,      // 可调整
}
```

方法：`check_core_values(action) -> Result<(), ValueViolation>`, `to_context_injection() -> CharacterContext`

### CoreValue

```rust
pub struct CoreValue {
    pub name: String,
    pub description: String,
    pub enforcement: ValueEnforcement,     // HardConstraint / SoftGuideline
    pub forbidden_keywords: Vec<String>,   // action.command 匹配
}
```

方法：`violates(action) -> bool`, 预设：`privacy_first()`, `honesty_first()`, `no_destructive_actions()`

### ValueEnforcement

```rust
pub enum ValueEnforcement {
    HardConstraint,  // 违反 → Err(ValueViolation)
    SoftGuideline,   // 警告 → Ok(())
}
```

### Preferences

```rust
pub struct Preferences {
    pub tool_preference: Vec<String>,
    pub risk_tolerance: f32,                // [0, 1]
    pub uncertainty_strategy: UncertaintyStrategy,
    pub output_style: OutputStyle,
}
```

### UncertaintyStrategy / OutputStyle

```rust
pub enum UncertaintyStrategy {
    SearchFirst,          // 先搜索更多信息
    AskUserFirst,         // 先问用户
    BestGuessAndConfirm,  // 最佳猜测 + 事后确认
}

pub enum OutputStyle {
    Concise,      // 简洁
    Detailed,     // 详细
    StepByStep,   // 逐步推理
}
```

## 内置核心价值观

| 预设 | 级别 | 禁止关键词 |
|---|---|---|
| `privacy_first()` | HardConstraint | leak, expose, share_pii, dump |
| `honesty_first()` | HardConstraint | fabricate, pretend_human, forge, impersonate |
| `no_destructive_actions()` | HardConstraint | delete_all, drop_table, rm_rf, format, shutdown, destroy |

## 目录结构

```
src/
├── lib.rs          // Character + CharacterContext + check_core_values() + tests
├── values.rs       // CoreValue + ValueEnforcement + ValueViolation + 3 个预设
└── preferences.rs  // Preferences + OutputStyle + UncertaintyStrategy
```

## 测试

```bash
cargo test -p agent-character
```

覆盖：HardConstraint 阻断/放行、SoftGuideline 不阻断、privacy/honesty 预设、context_injection。

## 依赖

- `agent-types-core` — Action
- `serde` — 序列化

## License

与仓库一致。
