//! McpClient —— MCP 工具调用客户端
//!
//! 实现 `agent_tools::ToolExecutor` trait，统一 MCP 工具协议。
//!
//! 两种模式：
//! - **mock**（默认）：本地模拟工具调用
//! - **http**（feature = "http"）：通过 HTTP POST 调用远程 MCP Server
//!
//! ## 使用
//! ```ignore
//! use agent_tools::{ToolExecutor, ToolCall, ToolDefinition};
//! let mut client = McpClient::new("http://mcp-server:8080");
//! client.register_tool("search");
//! // 通过 trait 调用（推荐）
//! let tools = client.list_tools().await;
//! let result = client.call(&ToolCall::new("c1", "search", json!({"q":"rust"}))).await;
//! // 或通过 Action 调用（兼容旧接口）
//! let result = client.call_action(&action).await;
//! ```

use agent_tools::{ToolCall, ToolDefinition, ToolExecutor, ToolResult};
use agent_types_core::{Action, ActionParams};
use serde::{Deserialize, Serialize};

/// MCP 工具调用结果（兼容旧接口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResult {
    pub tool_name: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

/// MCP 客户端 —— 调用外部 MCP Server 工具
///
/// 同时支持：
/// - `agent_tools::ToolExecutor` trait（标准 MCP 协议）
/// - `call_action(&Action) -> McpResult`（兼容旧接口）
pub struct McpClient {
    registered_tools: Vec<ToolDefinition>,
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

    /// 注册工具（仅名称，description/parameters 为空）
    pub fn register_tool(&mut self, tool_name: impl Into<String>) {
        self.registered_tools.push(ToolDefinition {
            name: tool_name.into(),
            description: String::new(),
            parameters: serde_json::Value::Null,
        });
    }

    /// 注册工具（完整定义，含 JSON Schema 参数）
    pub fn register_tool_full(&mut self, def: ToolDefinition) {
        self.registered_tools.push(def);
    }

    /// 调用 MCP 工具（兼容旧接口：Action → McpResult）
    pub async fn call_action(&self, action: &Action) -> McpResult {
        #[cfg(feature = "http")]
        {
            return self.call_action_http(action).await;
        }
        #[cfg(not(feature = "http"))]
        {
            return self.call_action_mock(action).await;
        }
    }

    /// 已注册工具数
    pub fn tool_count(&self) -> usize {
        self.registered_tools.len()
    }

    // ---- 内部实现 ----

    #[cfg(feature = "http")]
    async fn call_action_http(&self, action: &Action) -> McpResult {
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

    #[cfg(not(feature = "http"))]
    async fn call_action_mock(&self, action: &Action) -> McpResult {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let tool_name = action.command.clone();
        let known = self.registered_tools.iter().any(|t| t.name == tool_name);
        if !known {
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
}

// ===========================================================================
// ToolExecutor trait impl — 对接 agent-tools 统一协议
// ===========================================================================

/// 将 `ToolCall.arguments` 转换为 `ActionParams`。
/// 如果 arguments 是 JSON 对象 → 每个 key 作为参数；
/// 否则 → 放入 `"_args"` 键。
fn tool_args_to_action_params(args: &serde_json::Value) -> ActionParams {
    let mut params = ActionParams::new();
    match args {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                params = params.with(k.clone(), v.clone());
            }
        }
        other => {
            params = params.with("_args", other.clone());
        }
    }
    params
}

/// 将 `McpResult` → `ToolResult`
fn mcp_to_tool_result(call_id: String, r: McpResult) -> ToolResult {
    ToolResult {
        call_id,
        success: r.success,
        content: r.output.to_string(),
        error: r.error,
    }
}

#[async_trait::async_trait]
impl ToolExecutor for McpClient {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.registered_tools.clone()
    }

    async fn call(&self, call: &ToolCall) -> ToolResult {
        let action = Action::new(
            &call.name,
            tool_args_to_action_params(&call.arguments),
        );
        let result = self.call_action(&action).await;
        mcp_to_tool_result(call.id.clone(), result)
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client() -> McpClient {
        McpClient::new("http://localhost:8080")
    }

    // ---- ToolExecutor trait 测试 ----

    #[tokio::test]
    async fn list_tools_returns_registered() {
        let mut client = make_client();
        client.register_tool("search");
        client.register_tool_full(ToolDefinition {
            name: "fetch".into(),
            description: "Fetch a URL".into(),
            parameters: serde_json::json!({"url": {"type": "string"}}),
        });

        let tools = client.list_tools().await;
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "search");
        assert_eq!(tools[1].name, "fetch");
        assert_eq!(tools[1].description, "Fetch a URL");
    }

    #[tokio::test]
    async fn tool_call_object_args() {
        let mut client = make_client();
        client.register_tool("add");
        let call = ToolCall {
            id: "c1".into(),
            name: "add".into(),
            arguments: serde_json::json!({"a": 3, "b": 4}),
        };

        let result = client.call(&call).await;
        assert_eq!(result.call_id, "c1");
        #[cfg(not(feature = "http"))]
        assert!(result.success);
    }

    #[tokio::test]
    async fn tool_call_unregistered_fails() {
        let client = make_client();
        let call = ToolCall {
            id: "c2".into(),
            name: "nonexistent".into(),
            arguments: serde_json::Value::Null,
        };

        let result = client.call(&call).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn tool_call_scalar_args() {
        let mut client = make_client();
        client.register_tool("echo");
        // Non-object arguments → stored under "_args"
        let call = ToolCall {
            id: "c3".into(),
            name: "echo".into(),
            arguments: serde_json::json!("hello"),
        };

        let result = client.call(&call).await;
        #[cfg(not(feature = "http"))]
        assert!(result.success);
        #[cfg(feature = "http")]
        let _ = result; // may fail with no server
    }

    // ---- 兼容旧接口测试 ----

    #[tokio::test]
    async fn call_registered_tool_succeeds() {
        let mut client = make_client();
        client.register_tool("search");
        let action = Action::new("search", ActionParams::new().with("query", "rust"));
        let result = client.call_action(&action).await;
        #[cfg(not(feature = "http"))]
        assert!(result.success);
        #[cfg(feature = "http")]
        {
            if !result.success {
                assert!(result.error.is_some());
            }
        }
    }

    #[tokio::test]
    async fn call_unregistered_tool_fails() {
        let client = make_client();
        let action = Action::new("unknown", ActionParams::new());
        let result = client.call_action(&action).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn http_client_fails_gracefully_on_bad_url() {
        let mut client = McpClient::new("http://127.0.0.1:1");
        client.register_tool("test");
        let action = Action::new("test", ActionParams::new());
        let _result = client.call_action(&action).await; // must not panic
    }
}
