//! McpClient —— MCP 工具调用客户端

use agent_types_core::Action;
#[cfg(test)]
use agent_types_core::ActionParams;
use serde::{Deserialize, Serialize};

/// MCP 工具调用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResult {
    pub tool_name: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

/// MCP 客户端 —— 调用外部 MCP Server 工具
///
/// 当前为 mock 实现。生产环境通过 HTTP/gRPC 调用真实 MCP Server。
pub struct McpClient {
    /// 已注册的工具列表
    registered_tools: Vec<String>,
    /// MCP Server 地址
    #[allow(dead_code)]
    server_url: String,
}

impl McpClient {
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            registered_tools: Vec::new(),
            server_url: server_url.into(),
        }
    }

    /// 注册工具
    pub fn register_tool(&mut self, tool_name: impl Into<String>) {
        self.registered_tools.push(tool_name.into());
    }

    /// 调用 MCP 工具
    pub async fn call(&self, action: &Action) -> McpResult {
        // Mock: 模拟远程调用延迟
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let tool_name = action.command.clone();
        let is_registered = self.registered_tools.contains(&tool_name);

        if !is_registered {
            return McpResult {
                tool_name,
                success: false,
                output: serde_json::Value::Null,
                error: Some(format!("tool '{}' not registered", action.command)),
            };
        }

        // Mock success
        McpResult {
            tool_name,
            success: true,
            output: serde_json::json!({"status": "ok", "params": action.params.0}),
            error: None,
        }
    }

    /// 已注册工具数
    pub fn tool_count(&self) -> usize {
        self.registered_tools.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn call_registered_tool_succeeds() {
        let mut client = McpClient::new("http://localhost:8080");
        client.register_tool("search");
        let action = Action::new("search", ActionParams::new().with("query", "rust"));
        let result = client.call(&action).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn call_unregistered_tool_fails() {
        let client = McpClient::new("http://localhost:8080");
        let action = Action::new("unknown", ActionParams::new());
        let result = client.call(&action).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
