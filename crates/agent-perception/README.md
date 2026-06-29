# agent-perception

Agent **感知域** —— 输入解析 + PII 检测与处理 + ContextDescriptor 构建。

## 概述

Perception 是 Agent 能力域的第一环：将原始输入（文本/JSON/多模态）转换为结构化的 `ContextDescriptor`，同时检测并处理敏感信息（PII）。

```
RawInput → Parse → PII Scan → ContextDescriptor → AgentState
```

作为 visual_script NodeDefinition 注册：`"perception.observe"`（Impure + Async）。

## 特性

- **Perceiver trait** — 异步感知器接口，从原始输入生成 ContextDescriptor
- **PerceptionPipeline** — 可组合管道：`new()` / `with_pii()` / `run()` / `run_parsed()`
- **ParsedInput** — 结构化解析结果，支持文本和 JSON 输入
- **PiiScanner** — 5 种内置 PII 模式（email/phone/SSN/credit_card/IP）+ 3 种处理策略
- **PiiStrategy** — Mask（遮蔽）/ Encrypt（可逆加密标记）/ Remove（移除）
- **Regex 匹配** — 基于 regex 的模式检测，确定性、快速

## 安装

```toml
[dependencies]
agent-perception = { path = "../agent-perception" }
```

## 快速上手

### 基础感知管道

```rust
use agent_perception::PerceptionPipeline;

let pipeline = PerceptionPipeline::new();

// 无 PII 扫描，直接生成 ContextDescriptor
let ctx = pipeline.run("the user clicked the submit button").await;
println!("{}", ctx.description);
```

### 带 PII 扫描

```rust
use agent_perception::{PerceptionPipeline, PiiScanner, PiiStrategy};

let pipeline = PerceptionPipeline::new()
    .with_pii(PiiScanner::new(PiiStrategy::Mask));

let ctx = pipeline
    .run("user alice@example.com clicked submit")
    .await;

// PII 已遮蔽
assert!(ctx.description.contains("[email]"));
assert!(!ctx.description.contains("alice@example.com"));
```

### 结构化输入

```rust
use agent_perception::{PerceptionPipeline, ParsedInput, PiiScanner, PiiStrategy};

let parsed = ParsedInput::from_json(
    r#"{"name":"Alice","email":"alice@example.com"}"#,
    vec![("name".into(), "Alice".into())],
);

let pipeline = PerceptionPipeline::new()
    .with_pii(PiiScanner::new(PiiStrategy::Mask));

let ctx = pipeline.run_parsed(&parsed).await;
```

### 仅检测 PII

```rust
let scanner = PiiScanner::new(PiiStrategy::Mask);

if scanner.contains_pii("contact alice@example.com") {
    println!("PII detected — handle with care");
}
```

### 实现自定义 Perceiver

```rust
use agent_perception::{Perceiver, ContextDescriptor};
use async_trait::async_trait;

struct MyPerceiver;

#[async_trait]
impl Perceiver for MyPerceiver {
    async fn perceive(&self, raw_input: &str) -> ContextDescriptor {
        // 自定义解析逻辑
        ContextDescriptor::new(format!("parsed: {raw_input}"))
    }
}
```

## PII 处理策略

| 策略 | 效果 | 示例 |
|---|---|---|
| `Mask` | 替换为类型标签 | `alice@example.com` → `[email]` |
| `Encrypt` | 替换为加密标记 | `alice@example.com` → `[encrypted:email]` |
| `Remove` | 直接删除 | `alice@example.com` → (空) |

### 内置检测模式

| 模式 | Regex |
|---|---|
| email | `[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}` |
| phone | `\b\d{3}[-.]?\d{3}[-.]?\d{4}\b` |
| SSN | `\b\d{3}-\d{2}-\d{4}\b` |
| credit_card | `\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b` |
| ip_address | `\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b` |

## 核心类型

### Perceiver trait

```rust
#[async_trait]
pub trait Perceiver: Send + Sync {
    async fn perceive(&self, raw_input: &str) -> ContextDescriptor;
}
```

### PerceptionPipeline

```rust
pub struct PerceptionPipeline {
    pii_scanner: Option<PiiScanner>,
}
```

方法：`new()`, `with_pii(scanner)`, `run(raw_input)`, `run_parsed(parsed)`, `has_pii()`

### PiiScanner

```rust
pub struct PiiScanner { /* ... */ }
```

方法：`new(strategy)`, `scan_and_mask(text)`, `contains_pii(text)`, `strategy()`

### ParsedInput

```rust
pub struct ParsedInput {
    pub raw: String,
    pub text: String,
    pub fields: Vec<(String, String)>,
    pub is_structured: bool,
}
```

方法：`from_text(raw)`, `from_json(raw, fields)`

## 目录结构

```
src/
├── lib.rs       // Perceiver trait + PerceptionPipeline + tests
├── context.rs   // ContextDescriptor re-export + ParsedInput
└── pii.rs       // PiiScanner + PiiStrategy + tests
```

## 测试

```bash
cargo test -p agent-perception
```

覆盖：PII 三种策略 mask/remove/encrypt、contains_pii 检测、无 PII 原文不变、Pipeline 集成遮蔽、`run_parsed()` 结构化输入、ParsedInput 构造。

## 依赖

- `agent-state` — ContextDescriptor
- `agent-types-core` — 基础类型
- `regex` — PII 模式匹配
- `serde` + `serde_json` — 序列化
- `async-trait` + `tokio` — async trait + 运行时

## License

与仓库一致。
