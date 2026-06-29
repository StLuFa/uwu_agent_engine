//! # agent-execution
//!
//! 执行域 —— MCP 工具调用 + Guard 检查 + 可选 WASM 沙箱 + 输出格式化。
//!
//! 作为 visual_script NodeDefinition 注册：`"execution.act"`（Impure + Async）

mod mcp;
mod output;

#[cfg(feature = "wasm-sandbox")]
mod wasm_sandbox;

pub use mcp::McpClient;
pub use output::OutputFormatter;

use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

/// 执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub action: Action,
    pub success: bool,
    pub output: String,
    pub state_delta: Option<agent_state::StateDiff>,
    pub tokens_used: u64,
    pub time_elapsed_ms: u64,
}

/// 执行器 —— 执行具体动作
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, action: &Action, state: &AgentState) -> ExecutionResult;
}

/// 动作执行器 —— 调用链：Guard → MCP/WASM → 收集结果
pub struct ActionExecutor {
    mcp_client: Option<McpClient>,
    max_parallel_actions: usize,
    action_timeout_ms: u64,
}

impl ActionExecutor {
    pub fn new() -> Self {
        Self {
            mcp_client: None,
            max_parallel_actions: 8,
            action_timeout_ms: 30000,
        }
    }

    pub fn with_mcp(mut self, client: McpClient) -> Self {
        self.mcp_client = Some(client);
        self
    }

    pub fn with_max_parallel(mut self, n: usize) -> Self {
        self.max_parallel_actions = n;
        self
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}
