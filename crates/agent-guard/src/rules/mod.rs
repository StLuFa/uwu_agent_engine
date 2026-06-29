//! Built-in guard rules

use crate::{GuardViolation, ViolationLevel};
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

// ===========================================================================
// Instruction Rules
// ===========================================================================

/// 禁止递归删除
pub struct NoRmRfRule;

#[async_trait]
impl crate::InstructionRule for NoRmRfRule {
    async fn check(&self, action: &Action) -> Option<GuardViolation> {
        let cmd = action.command.to_lowercase();
        if cmd.contains("rm_rf")
            || cmd.contains("rm -rf")
            || cmd.contains("delete_all")
            || cmd.contains("drop_table")
            || cmd.contains("format")
        {
            return Some(GuardViolation {
                rule: "no-rm-rf".into(),
                level: ViolationLevel::Critical,
                message: format!("destructive command blocked: {}", action.command),
            });
        }
        None
    }
}

/// 禁止执行 shell 命令
pub struct NoShellExecutionRule;

#[async_trait]
impl crate::InstructionRule for NoShellExecutionRule {
    async fn check(&self, action: &Action) -> Option<GuardViolation> {
        let cmd = action.command.to_lowercase();
        if cmd.contains("exec") || cmd.contains("system") || cmd.contains("shell") {
            return Some(GuardViolation {
                rule: "no-shell-exec".into(),
                level: ViolationLevel::Critical,
                message: format!("shell execution blocked: {}", action.command),
            });
        }
        None
    }
}

// ===========================================================================
// Parameter Rules
// ===========================================================================

/// 文件大小限制
pub struct FileSizeLimitRule {
    pub max_bytes: u64,
}

#[async_trait]
impl crate::ParameterRule for FileSizeLimitRule {
    async fn check(&self, _action: &Action, params: &ActionParams) -> Option<GuardViolation> {
        if let Some(size) = params.get("size").and_then(|v| v.as_u64()) {
            if size > self.max_bytes {
                return Some(GuardViolation {
                    rule: "file-size-limit".into(),
                    level: ViolationLevel::Critical,
                    message: format!("file size {size} exceeds max {}", self.max_bytes),
                });
            }
        }
        None
    }
}

/// 端口白名单
pub struct PortAllowlistRule {
    pub allowed_ports: Vec<u16>,
}

#[async_trait]
impl crate::ParameterRule for PortAllowlistRule {
    async fn check(&self, _action: &Action, params: &ActionParams) -> Option<GuardViolation> {
        if let Some(port) = params.get("port").and_then(|v| v.as_u64()) {
            let port = port as u16;
            if !self.allowed_ports.contains(&port) {
                return Some(GuardViolation {
                    rule: "port-allowlist".into(),
                    level: ViolationLevel::Critical,
                    message: format!("port {port} not in allowlist"),
                });
            }
        }
        None
    }
}

// ===========================================================================
// Budget Rules
// ===========================================================================

/// Token 预算检查
pub struct TokenBudgetRule;

#[async_trait]
impl crate::BudgetRule for TokenBudgetRule {
    async fn check(
        &self,
        tokens_used: u64,
        max_tokens: u64,
        _retries: u32,
        _max_retries: u32,
    ) -> Option<GuardViolation> {
        if tokens_used >= max_tokens {
            return Some(GuardViolation {
                rule: "token-budget".into(),
                level: ViolationLevel::Critical,
                message: format!("token budget exhausted: {tokens_used}/{max_tokens}"),
            });
        }
        None
    }
}

/// 重试次数限制
pub struct RetryBudgetRule;

#[async_trait]
impl crate::BudgetRule for RetryBudgetRule {
    async fn check(
        &self,
        _tokens_used: u64,
        _max_tokens: u64,
        retries: u32,
        max_retries: u32,
    ) -> Option<GuardViolation> {
        if retries > max_retries {
            return Some(GuardViolation {
                rule: "retry-budget".into(),
                level: ViolationLevel::Critical,
                message: format!("retry budget exhausted: {retries}/{max_retries}"),
            });
        }
        None
    }
}

// ===========================================================================
// Egress Rules
// ===========================================================================

/// MCP 写入白名单
pub struct McpWriteAllowlistRule {
    pub allowed_targets: Vec<String>,
}

#[async_trait]
impl crate::EgressRule for McpWriteAllowlistRule {
    async fn check_egress(&self, target: &str) -> Option<GuardViolation> {
        if !self.allowed_targets.iter().any(|t| target.contains(t)) {
            return Some(GuardViolation {
                rule: "mcp-write-allowlist".into(),
                level: ViolationLevel::Critical,
                message: format!("MCP write target not allowed: {target}"),
            });
        }
        None
    }
}

/// 禁止访问内网地址
pub struct NoNetworkToInternalRule;

#[async_trait]
impl crate::EgressRule for NoNetworkToInternalRule {
    async fn check_egress(&self, target: &str) -> Option<GuardViolation> {
        if target.contains("10.") || target.contains("192.168.") || target.contains("172.16.") {
            return Some(GuardViolation {
                rule: "no-internal-network".into(),
                level: ViolationLevel::Critical,
                message: format!("internal network access blocked: {target}"),
            });
        }
        None
    }
}

// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InstructionRule, ParameterRule, BudgetRule, EgressRule};
    use agent_types_core::ActionParams;

    #[tokio::test]
    async fn no_rm_rf_blocks_destructive() {
        let rule = NoRmRfRule;
        let action = Action::new("rm_rf", ActionParams::new());
        assert!(rule.check(&action).await.is_some());
    }

    #[tokio::test]
    async fn no_rm_rf_allows_safe() {
        let rule = NoRmRfRule;
        let action = Action::new("click", ActionParams::new());
        assert!(rule.check(&action).await.is_none());
    }

    #[tokio::test]
    async fn token_budget_blocks_when_exhausted() {
        let rule = TokenBudgetRule;
        assert!(rule.check(1000, 1000, 0, 5).await.is_some());
        assert!(rule.check(500, 1000, 0, 5).await.is_none());
    }

    #[tokio::test]
    async fn mcp_write_allowlist_blocks_unknown() {
        let rule = McpWriteAllowlistRule {
            allowed_targets: vec!["safe-server".into()],
        };
        assert!(rule.check_egress("evil-server").await.is_some());
        assert!(rule.check_egress("safe-server/api").await.is_none());
    }

    #[tokio::test]
    async fn port_allowlist_blocks_unlisted() {
        let rule = PortAllowlistRule {
            allowed_ports: vec![80, 443],
        };
        let action = Action::new("connect", ActionParams::new().with("port", 8080));
        let params = ActionParams::new().with("port", 8080);
        assert!(rule.check(&action, &params).await.is_some());

        let params_ok = ActionParams::new().with("port", 443);
        assert!(rule.check(&action, &params_ok).await.is_none());
    }

    #[tokio::test]
    async fn no_internal_network_blocks_private_ips() {
        let rule = NoNetworkToInternalRule;
        assert!(rule.check_egress("http://10.0.0.1/api").await.is_some());
        assert!(rule.check_egress("http://192.168.1.1").await.is_some());
        assert!(rule.check_egress("http://google.com").await.is_none());
    }
}
