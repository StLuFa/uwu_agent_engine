# agent-tools

MCP (Model Context Protocol) 工具调用抽象。定义工具类型和执行器 trait，支持 mock 和 HTTP 两种模式。

## 类型

| 类型 | 说明 |
|---|---|
| `ToolDefinition` | 工具定义（名称 + 描述 + JSON Schema 参数） |
| `ToolCall` | 工具调用请求（ID + 名称 + 参数） |
| `ToolResult` | 工具调用结果（成功/失败 + 输出 + 错误） |
| `ToolExecutor` | 异步 trait：`list_tools()` + `call()` |

## 使用

```rust
use agent_tools::{ToolCall, ToolDefinition, ToolExecutor};

let call = ToolCall::new("c1", "search", serde_json::json!({"q": "rust"}));
// Execute via ToolExecutor trait implementation
```

## 功能

- `http` feature：通过 reqwest 调用远程 MCP Server
- 默认 mode：mock 模式（本地模拟工具调用）

## 消费者

- `agent-execution` — 通过 McpClient 实现 ToolExecutor

## 依赖

- `agent-types-core` — 基础类型
- `agent-types-ext` — 扩展类型（AgentCard / TaskManifest）
