//! McpClient —— MCP 工具调用客户端
//!
//! 两种模式：
//! - **mock**（默认）：本地模拟工具调用，返回确定性结果
//! - **http**（feature = "http"）：通过 HTTP POST 调用远程 MCP Server
//!
//! ## 使用
//! ```ignore
//! let client = McpClient::new("http://mcp-server:8080")
//!     .register_tool("search");
//! let result = client.call(&action).await;
//! ```

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
pub struct McpClient {
    /// 已注册的工具列表（mock 模式用于验证；http 模式由服务端验证）
    registered_tools: Vec<String>,
    /// MCP Server 地址（mock 模式下仅存储）
    #[cfg_attr(not(feature = "http"), allow(dead_code))]
    server_url: String,
}

impl McpClient {
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            registered_tools: Vec::new(),
            server_url: server_url.into(),
        }
    }

    /// 注册工具（mock 模式下用于工具名校验）
    pub fn register_tool(&mut self, tool_name: impl Into<String>) {
        self.registered_tools.push(tool_name.into());
    }

    /// 调用 MCP 工具
    ///
    /// 启用 `http` feature 时通过 HTTP POST 发送 JSON-RPC 风格请求，
    /// 否则使用本地 mock。
    pub async fn call(&self, action: &Action) -> McpResult {
        #[cfg(feature = "http")]
        {
            return self.call_http(action).await;
        }
        #[cfg(not(feature = "http"))]
        {
            return self.call_mock(action).await;
        }
    }

    /// HTTP 实现：POST {server_url}/tools/call
    #[cfg(feature = "http")]
    async fn call_http(&self, action: &Action) -> McpResult {
        let tool_name = action.command.clone();
        let body = serde_json::json!({
            "tool": tool_name,
            "params": action.params.0,
        });

        match reqwest::Client::new()
            .post(format!("{}/tools/call", self.server_url))
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => {
                    let success = json
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    McpResult {
                        tool_name,
                        success,
                        output: json.get("output").cloned().unwrap_or(serde_json::Value::Null),
                        error: if success {
                            None
                        } else {
                            json.get("error")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        },
                    }
                }
                Err(e) => McpResult {
                    tool_name,
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some(format!("response parse error: {e}")),
                },
            },
            Err(e) => McpResult {
                tool_name,
                success: false,
                output: serde_json::Value::Null,
                error: Some(format!("http error: {e}")),
            },
        }
    }

    /// Mock 实现：模拟远程调用（feature = "http" 未启用时）
    #[cfg(not(feature = "http"))]
    async fn call_mock(&self, action: &Action) -> McpResult {
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

    /// In mock mode: succeeds. In HTTP mode with no server: fails gracefully.
    #[tokio::test]
    async fn call_registered_tool_succeeds() {
        let mut client = McpClient::new("http://localhost:8080");
        client.register_tool("search");
        let action = Action::new("search", ActionParams::new().with("query", "rust"));
        let result = client.call(&action).await;
        #[cfg(not(feature = "http"))]
        assert!(result.success);
        #[cfg(feature = "http")]
        {
            // With real HTTP and no server running, expect a connection error
            if !result.success {
                assert!(result.error.is_some());
            }
        }
    }

    /// In mock mode: fails on unregistered tool. In HTTP mode: also fails.
    #[tokio::test]
    async fn call_unregistered_tool_fails() {
        let client = McpClient::new("http://localhost:8080");
        let action = Action::new("unknown", ActionParams::new());
        let result = client.call(&action).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    /// Verifies graceful failure with a bad URL — no panic.
    #[tokio::test]
    async fn http_client_fails_gracefully_on_bad_url() {
        let mut client = McpClient::new("http://127.0.0.1:1"); // nothing listening
        client.register_tool("test");
        let action = Action::new("test", ActionParams::new());
        let _result = client.call(&action).await; // must not panic
    }
}
