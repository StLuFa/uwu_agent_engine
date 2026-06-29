//! # agent-tools
//!
//! MCP 工具协议 —— ToolExecutor + MCPClient。
//! 优先 MCP（Model Context Protocol），兼容 OpenAI Function Calling。

use serde::{Deserialize, Serialize};

/// 工具定义（MCP / Function Calling 统一表示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// 工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// 工具调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub success: bool,
    pub content: String,
    pub error: Option<String>,
}

/// 工具执行器 —— 执行工具调用
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn list_tools(&self) -> Vec<ToolDefinition>;
    async fn call(&self, call: &ToolCall) -> ToolResult;
}
