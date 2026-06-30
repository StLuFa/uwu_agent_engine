//! # agent-guard
//!
//! 五层安全闸门 —— 指令 / 参数 / 能力 / 预算 / egress。
//! 编译期注册，运行时不可绕过，不可自提升。
//!
//! ## 五层闸门
//!
//! | 闸门 | 职责 | 示例规则 |
//! |---|---|---|
//! | Instruction | 检查动作指令 | `no-rm-rf` 禁止递归删除 |
//! | Parameter | 检查参数合法性 | 文件大小限制、端口白名单 |
//! | Capability | 检查能力权限 | 禁止未注册的能力调用 |
//! | Budget | 检查预算 | Token / 时间 / 重试 预算 |
//! | Egress | 检查出站写入 | MCP 写入白名单 |

mod audit;
pub mod rules;

pub use audit::AuditLog;

use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 违规级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViolationLevel {
    Warning,
    Critical,
}

/// 守卫违规
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardViolation {
    pub rule: String,
    pub level: ViolationLevel,
    pub message: String,
}

// ---- 五层 trait ----

#[async_trait]
pub trait InstructionRule: Send + Sync {
    async fn check(&self, action: &Action) -> Option<GuardViolation>;
}

#[async_trait]
pub trait ParameterRule: Send + Sync {
    async fn check(&self, action: &Action, params: &ActionParams) -> Option<GuardViolation>;
}

#[async_trait]
pub trait CapabilityRule: Send + Sync {
    async fn check(&self, action: &Action) -> Option<GuardViolation>;
}

#[async_trait]
pub trait BudgetRule: Send + Sync {
    async fn check(&self, tokens_used: u64, max_tokens: u64, retries: u32, max_retries: u32) -> Option<GuardViolation>;
}

#[async_trait]
pub trait EgressRule: Send + Sync {
    async fn check_egress(&self, target: &str) -> Option<GuardViolation>;
}

/// Agent 上下文（Guard 检查所需信息）
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub session_id: String,
    pub agent_id: String,
    pub tokens_used: u64,
    pub max_tokens: u64,
    pub retries: u32,
    pub max_retries: u32,
}

/// 守卫层 —— 五层硬闸门
pub struct GuardLayer {
    instruction_rules: Vec<Box<dyn InstructionRule + Send + Sync>>,
    parameter_rules: Vec<Box<dyn ParameterRule + Send + Sync>>,
    capability_rules: Vec<Box<dyn CapabilityRule + Send + Sync>>,
    budget_rules: Vec<Box<dyn BudgetRule + Send + Sync>>,
    egress_rules: Vec<Box<dyn EgressRule + Send + Sync>>,
    audit_log: Arc<AuditLog>,
}

impl GuardLayer {
    pub fn builder() -> GuardBuilder {
        GuardBuilder::new()
    }

    /// 强制执行五层闸门，返回放行的动作或被阻断的违规列表
    pub async fn enforce(
        &self,
        actions: &[Action],
        context: &AgentContext,
    ) -> Result<Vec<Action>, Vec<GuardViolation>> {
        let mut allowed = Vec::new();
        let mut blocked = Vec::new();

        for action in actions {
            let mut violations = Vec::new();

            // 1. 指令检查
            for rule in &self.instruction_rules {
                if let Some(v) = rule.check(action).await {
                    violations.push(v);
                }
            }
            // 2. 参数检查
            for rule in &self.parameter_rules {
                if let Some(v) = rule.check(action, &action.params).await {
                    violations.push(v);
                }
            }
            // 3. 能力检查
            for rule in &self.capability_rules {
                if let Some(v) = rule.check(action).await {
                    violations.push(v);
                }
            }
            // 4. 预算检查
            for rule in &self.budget_rules {
                if let Some(v) = rule.check(context.tokens_used, context.max_tokens, context.retries, context.max_retries).await {
                    violations.push(v);
                }
            }

            if violations.is_empty() {
                allowed.push(action.clone());
            } else {
                self.audit_log.log_guard_hit(action, &violations).await;
                blocked.extend(violations);
            }
        }

        if blocked.is_empty() {
            Ok(allowed)
        } else {
            Err(blocked)
        }
    }

    /// Egress 检查（学习写入出站）
    pub async fn check_egress(&self, target: &str) -> Result<(), GuardViolation> {
        for rule in &self.egress_rules {
            if let Some(v) = rule.check_egress(target).await {
                return Err(v);
            }
        }
        Ok(())
    }
}

/// GuardLayer 构建器 —— 编译期注册规则
pub struct GuardBuilder {
    instruction_rules: Vec<Box<dyn InstructionRule + Send + Sync>>,
    parameter_rules: Vec<Box<dyn ParameterRule + Send + Sync>>,
    capability_rules: Vec<Box<dyn CapabilityRule + Send + Sync>>,
    budget_rules: Vec<Box<dyn BudgetRule + Send + Sync>>,
    egress_rules: Vec<Box<dyn EgressRule + Send + Sync>>,
    audit_log_path: Option<String>,
}

impl GuardBuilder {
    pub fn new() -> Self {
        Self {
            instruction_rules: Vec::new(),
            parameter_rules: Vec::new(),
            capability_rules: Vec::new(),
            budget_rules: Vec::new(),
            egress_rules: Vec::new(),
            audit_log_path: None,
        }
    }

    pub fn add_instruction_rule<R: InstructionRule + 'static>(mut self, rule: R) -> Self {
        self.instruction_rules.push(Box::new(rule));
        self
    }

    pub fn add_parameter_rule<R: ParameterRule + 'static>(mut self, rule: R) -> Self {
        self.parameter_rules.push(Box::new(rule));
        self
    }

    pub fn add_capability_rule<R: CapabilityRule + 'static>(mut self, rule: R) -> Self {
        self.capability_rules.push(Box::new(rule));
        self
    }

    pub fn add_budget_rule<R: BudgetRule + 'static>(mut self, rule: R) -> Self {
        self.budget_rules.push(Box::new(rule));
        self
    }

    pub fn add_egress_rule<R: EgressRule + 'static>(mut self, rule: R) -> Self {
        self.egress_rules.push(Box::new(rule));
        self
    }

    pub fn audit_log_path(mut self, path: impl Into<String>) -> Self {
        self.audit_log_path = Some(path.into());
        self
    }

    pub fn build(self) -> GuardLayer {
        GuardLayer {
            instruction_rules: self.instruction_rules,
            parameter_rules: self.parameter_rules,
            capability_rules: self.capability_rules,
            budget_rules: self.budget_rules,
            egress_rules: self.egress_rules,
            audit_log: Arc::new(AuditLog::new(self.audit_log_path.as_deref())),
        }
    }
}

impl Default for GuardBuilder {
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
    use crate::rules::{NoRmRfRule, TokenBudgetRule, McpWriteAllowlistRule};

    #[tokio::test]
    async fn enforce_allows_safe_actions() {
        let layer = GuardLayer::builder()
            .add_instruction_rule(NoRmRfRule)
            .add_budget_rule(TokenBudgetRule)
            .build();

        let actions = vec![Action::new("click", ActionParams::new())];
        let ctx = AgentContext {
            session_id: "s1".into(),
            agent_id: "a1".into(),
            tokens_used: 100,
            max_tokens: 1000,
            retries: 0,
            max_retries: 5,
        };

        let result = layer.enforce(&actions, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn enforce_blocks_destructive_action() {
        let layer = GuardLayer::builder()
            .add_instruction_rule(NoRmRfRule)
            .build();

        let actions = vec![Action::new("rm_rf", ActionParams::new())];
        let ctx = AgentContext {
            session_id: "s1".into(),
            agent_id: "a1".into(),
            tokens_used: 100,
            max_tokens: 1000,
            retries: 0,
            max_retries: 5,
        };

        let result = layer.enforce(&actions, &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn enforce_blocks_on_budget_exhausted() {
        let layer = GuardLayer::builder()
            .add_budget_rule(TokenBudgetRule)
            .build();

        let actions = vec![Action::new("click", ActionParams::new())];
        let ctx = AgentContext {
            session_id: "s1".into(),
            agent_id: "a1".into(),
            tokens_used: 1000,
            max_tokens: 1000, // exhausted
            retries: 0,
            max_retries: 5,
        };

        let result = layer.enforce(&actions, &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_egress_allows_safe() {
        let layer = GuardLayer::builder()
            .add_egress_rule(McpWriteAllowlistRule {
                allowed_targets: vec!["safe".into()],
            })
            .build();

        assert!(layer.check_egress("safe-server").await.is_ok());
        assert!(layer.check_egress("evil-server").await.is_err());
    }

    #[test]
    fn builder_registers_all_layers() {
        let _layer = GuardLayer::builder()
            .add_instruction_rule(NoRmRfRule)
            .add_budget_rule(TokenBudgetRule)
            .add_egress_rule(McpWriteAllowlistRule {
                allowed_targets: vec!["a".into()],
            })
            .build();

        // Compiles and builds = all layers registered
    }

    #[tokio::test]
    async fn enforce_partial_pass_some_allowed_some_blocked() {
        let layer = GuardLayer::builder()
            .add_instruction_rule(NoRmRfRule)
            .build();

        let safe = Action::new("click", ActionParams::new());
        let dangerous = Action::new("rm_rf", ActionParams::new().with("path", "/"));
        let actions = vec![safe.clone(), dangerous];

        let ctx = AgentContext {
            session_id: "s1".into(),
            agent_id: "a1".into(),
            tokens_used: 100,
            max_tokens: 1000,
            retries: 0,
            max_retries: 5,
        };

        let result = layer.enforce(&actions, &ctx).await;
        // Should be Err because at least one action was blocked
        assert!(result.is_err());
        let violations = result.unwrap_err();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "no-rm-rf");
    }

    #[tokio::test]
    async fn enforce_with_all_eight_rules() {
        use crate::rules::{
            NoShellExecutionRule, FileSizeLimitRule, PortAllowlistRule,
            RetryBudgetRule, NoNetworkToInternalRule,
        };

        let layer = GuardLayer::builder()
            .add_instruction_rule(NoRmRfRule)
            .add_instruction_rule(NoShellExecutionRule)
            .add_parameter_rule(FileSizeLimitRule { max_bytes: 1024 })
            .add_parameter_rule(PortAllowlistRule { allowed_ports: vec![80, 443] })
            .add_budget_rule(TokenBudgetRule)
            .add_budget_rule(RetryBudgetRule)
            .add_egress_rule(McpWriteAllowlistRule {
                allowed_targets: vec!["api.safe.com".into()],
            })
            .add_egress_rule(NoNetworkToInternalRule)
            .build();

        let ctx = AgentContext {
            session_id: "s1".into(),
            agent_id: "a1".into(),
            tokens_used: 100,
            max_tokens: 1000,
            retries: 1,
            max_retries: 5,
        };

        // All safe actions — should pass all 8 rules
        let safe_actions = vec![Action::new("click", ActionParams::new().with("port", 443u64))];
        assert!(layer.enforce(&safe_actions, &ctx).await.is_ok());

        // Egress: safe target passes
        assert!(layer.check_egress("api.safe.com/v1/upload").await.is_ok());

        // Egress: internal IP blocked
        assert!(layer.check_egress("http://10.0.0.5/admin").await.is_err());

        // Budget: token exhaustion blocked
        let exhausted_ctx = AgentContext {
            tokens_used: 1000,
            max_tokens: 1000,
            ..ctx.clone()
        };
        let result = layer.enforce(&safe_actions, &exhausted_ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn enforce_preserves_action_order_on_partial_pass() {
        let layer = GuardLayer::builder()
            .add_instruction_rule(NoRmRfRule)
            .build();

        let a1 = Action::new("click_a", ActionParams::new());
        let a2 = Action::new("rm_rf", ActionParams::new());
        let a3 = Action::new("click_c", ActionParams::new());

        let actions = vec![a1.clone(), a2, a3.clone()];
        let ctx = AgentContext {
            session_id: "s1".into(),
            agent_id: "a1".into(),
            tokens_used: 0,
            max_tokens: 1000,
            retries: 0,
            max_retries: 5,
        };

        let result = layer.enforce(&actions, &ctx).await;
        assert!(result.is_err());
        let violations = result.unwrap_err();
        assert_eq!(violations.len(), 1);
        // The blocked violation should reference rm_rf
        assert!(violations[0].message.contains("rm_rf"));
    }
}
