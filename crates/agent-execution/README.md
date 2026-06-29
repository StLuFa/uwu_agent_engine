# agent-execution

Agent **执行域** —— MCP 工具调用 + 输出格式化 + 动作执行。

## 概述

Execution 是 Agent 决策循环的最后一环：接收 `Reasoning → Decision`，调用 MCP 工具执行动作，收集结果并格式化输出。

```
Decision.action → [Guard 检查] → McpClient.call() → ExecutionResult → OutputFormatter
                                                      ↓
                                              state_delta（修正 State）
```

作为 visual_script NodeDefinition 注册：`"execution.act"`（Impure + Async）。

## 特性

- **Executor trait** — 异步执行器接口，execute(action, state) → ExecutionResult
- **ActionExecutor** — 调用链：Guard → MCP → 结果收集，支持批量执行
- **McpClient** — MCP 工具调用客户端（当前 mock），工具注册 + 调用
- **OutputFormatter** — 三种输出格式：PlainText / Json / Markdown
- **并行控制** — `max_parallel_actions` 限制并发执行数

## 安装

```toml
[dependencies]
agent-execution = { path = "../agent-execution" }
```

## 快速上手

### 执行动作

```rust
use agent_execution::ActionExecutor;
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};

let executor = ActionExecutor::new();
let state = AgentState::new();

let action = Action::new("click", ActionParams::new().with("target", "#submit"));
let result = executor.execute_action(&action, &state).await;

println!("success: {}, time: {}ms", result.success, result.time_elapsed_ms);
```

### 带 MCP 工具调用

```rust
use agent_execution::{ActionExecutor, McpClient};

let mut mcp = McpClient::new("http://localhost:8080");
mcp.register_tool("search");
mcp.register_tool("click");

let executor = ActionExecutor::new().with_mcp(mcp);

let action = Action::new("search", ActionParams::new().with("query", "rust"));
let result = executor.execute_action(&action, &state).await;
```

### 批量执行

```rust
let actions = vec![
    Action::new("search", ActionParams::new().with("query", "a")),
    Action::new("click", ActionParams::new().with("target", "btn")),
    Action::new("type", ActionParams::new().with("text", "hello")),
];

let executor = ActionExecutor::new().with_max_parallel(2);
let results = executor.execute_batch(&actions, &state).await;
// → max 2 actions executed
```

### 输出格式化

```rust
use agent_execution::{OutputFormatter, OutputFormat};

let formatter = OutputFormatter::new(OutputFormat::Markdown);
let output = formatter.format_result("click", true, "button clicked", 42);
// → **`click`** [OK] (42ms)\n\nbutton clicked

let json = OutputFormatter::new(OutputFormat::Json)
    .format_result("search", true, "5 results", 100);
// → {"status":"OK","action":"search","output":"5 results","time_ms":100}
```

### 实现自定义 Executor

```rust
use agent_execution::{Executor, ExecutionResult};
use agent_state::AgentState;
use agent_types_core::Action;
use async_trait::async_trait;

struct MyExecutor;

#[async_trait]
impl Executor for MyExecutor {
    async fn execute(&self, action: &Action, state: &AgentState) -> ExecutionResult {
        ExecutionResult {
            action: action.clone(),
            success: true,
            output: format!("done: {}", action.command),
            state_delta: None,
            tokens_used: 0,
            time_elapsed_ms: 5,
        }
    }
}
```

## 核心类型

### ExecutionResult

```rust
pub struct ExecutionResult {
    pub action: Action,
    pub success: bool,
    pub output: String,
    pub state_delta: Option<StateDiff>,  // 执行后的状态变化
    pub tokens_used: u64,
    pub time_elapsed_ms: u64,
}
```

### Executor trait

```rust
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, action: &Action, state: &AgentState) -> ExecutionResult;
}
```

### ActionExecutor

```rust
pub struct ActionExecutor { /* ... */ }
```

方法：`new()`, `with_mcp(client)`, `with_max_parallel(n)`, `execute_action(action, state)`, `execute_batch(actions, state)`

### McpClient

```rust
pub struct McpClient { /* ... */ }
```

方法：`new(server_url)`, `register_tool(name)`, `call(action) -> McpResult`, `tool_count()`

### OutputFormatter

```rust
pub struct OutputFormatter { format: OutputFormat }
```

方法：`new(format)`, `format(content)`, `format_result(action, success, output, time_ms)`, `current_format()`

### OutputFormat

```rust
pub enum OutputFormat { PlainText, Json, Markdown }
```

## 目录结构

```
src/
├── lib.rs      // ExecutionResult + Executor trait + ActionExecutor + tests
├── mcp.rs      // McpClient + McpResult + tests
└── output.rs   // OutputFormatter + OutputFormat + tests
```

## 后续集成

- 接 `agent-guard` → Guard 检查嵌入 execute_action 调用链
- 接真实 MCP Server → HTTP/gRPC 替换 mock
- 接 `uwu_wasm` → WASM 沙箱执行不可信代码（feature flag）

## 测试

```bash
cargo test -p agent-execution
```

覆盖：MCP 工具注册/调用/未注册失败、输出三种格式、ActionExecutor 单步执行、批量并行限制、默认配置。

## 依赖

- `agent-state` — AgentState + StateDiff
- `agent-types-core` — Action/ActionParams
- `async-trait` — async trait
- `serde` + `serde_json` + `chrono` — 序列化

## License

与仓库一致。
