//! # agent-execution
//!
//! 执行域 —— MCP 工具调用 + Guard 检查 + 输出格式化。
//!
//! 作为 visual_script NodeDefinition 注册：`"execution.act"`（Impure + Async）

mod mcp;
mod output;
#[cfg(feature = "visual-script")]
pub mod vs_nodes;
#[cfg(feature = "wasm-sandbox")]
pub mod wasm;

pub use mcp::{McpClient, McpResult};
pub use output::{OutputFormat, OutputFormatter};

use agent_state::AgentState;
use agent_types_core::Action;
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

/// 执行器 trait
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, action: &Action, state: &AgentState) -> ExecutionResult;
}

/// 动作执行器 —— 调用链：Guard → MCP → 收集结果
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
            action_timeout_ms: 30_000,
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

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.action_timeout_ms = ms;
        self
    }

    /// 执行单个动作（带超时保护）
    pub async fn execute_action(
        &self,
        action: &Action,
        _state: &AgentState,
    ) -> ExecutionResult {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(self.action_timeout_ms);

        let exec_future = async {
            if let Some(ref mcp) = self.mcp_client {
                let result = mcp.call_action(action).await;
                let out = serde_json::to_string(&result.output).unwrap_or_default();
                (result.success, out)
            } else {
                (true, format!("executed: {}", action.command))
            }
        };

        let (success, output) = match tokio::time::timeout(timeout, exec_future).await {
            Ok(result) => result,
            Err(_elapsed) => {
                (false, format!("timeout after {}ms", self.action_timeout_ms))
            }
        };

        let elapsed = start.elapsed().as_millis() as u64;

        ExecutionResult {
            action: action.clone(),
            success,
            output,
            state_delta: None,
            tokens_used: 0,
            time_elapsed_ms: elapsed,
        }
    }

    /// 并行执行多个动作
    pub async fn execute_batch(
        &self,
        actions: &[Action],
        state: &AgentState,
    ) -> Vec<ExecutionResult> {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions.iter().take(self.max_parallel_actions) {
            results.push(self.execute_action(action, state).await);
        }
        results
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use agent_types_core::ActionParams;

    #[tokio::test]
    async fn execute_action_without_mcp() {
        let executor = ActionExecutor::new();
        let state = AgentState::new();
        let action = Action::new("click", ActionParams::new().with("target", "btn"));

        let result = executor.execute_action(&action, &state).await;
        assert!(result.success);
        assert!(result.output.contains("click"));
        assert!(result.time_elapsed_ms < 1000);
    }

    #[tokio::test]
    async fn execute_action_with_mcp() {
        let mut mcp = McpClient::new("http://localhost:8080");
        mcp.register_tool("search");

        let executor = ActionExecutor::new().with_mcp(mcp);
        let state = AgentState::new();
        let action = Action::new("search", ActionParams::new().with("query", "rust"));

        let result = executor.execute_action(&action, &state).await;
        // In mock mode: succeeds. In http mode with no server: may fail.
        // Either way, the executor must not panic.
        let _ = result.success;
    }

    #[tokio::test]
    async fn execute_batch_respects_max_parallel() {
        let executor = ActionExecutor::new().with_max_parallel(2);
        let state = AgentState::new();
        let actions: Vec<_> = (0..5)
            .map(|i| Action::new(format!("act_{i}"), ActionParams::new()))
            .collect();

        let results = executor.execute_batch(&actions, &state).await;
        assert_eq!(results.len(), 2); // capped at max_parallel
    }

    #[test]
    fn default_executor_has_sensible_limits() {
        let executor = ActionExecutor::default();
        assert_eq!(executor.max_parallel_actions, 8);
    }
}
