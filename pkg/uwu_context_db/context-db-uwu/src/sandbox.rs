//! 写入安全沙箱（F25）—— 在 CharacterConstraint 关键词检查之上叠加 LLM 语义审查。
//!
//! 沙箱流程：
//! 1. CharacterConstraint 关键词快速拦截（同步，零 LLM 调用）
//! 2. SemanticSandbox LLM 语义审查（异步，深度检查）
//! 3. 通过 → 持久化 / 拒绝 → 写入 `.quarantine` 隔离区

use agent_context_db_core::{ContextEntry, LlmClient, LlmOpts, Result};
use std::sync::Arc;

/// 沙箱审查结果。
#[derive(Debug, Clone)]
pub enum SandboxVerdict {
    /// 通过
    Pass,
    /// 拒绝（附原因）
    Reject { reason: String, rule: String },
    /// 隔离（不确定，人工审核）
    Quarantine { reason: String, risk_score: f32 },
}

/// 语义沙箱 —— LLM 驱动的深度内容审查。
pub struct SemanticSandbox {
    llm: Arc<dyn LlmClient>,
    /// 安全策略
    policies: Vec<SafetyPolicy>,
}

#[derive(Debug, Clone)]
pub struct SafetyPolicy {
    pub name: String,
    /// 策略描述（给 LLM 的 prompt 用）
    pub description: String,
    /// 风险等级 0-1（越高越严格）
    pub severity: f32,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self {
            name: "default".into(),
            description: "general safety check".into(),
            severity: 0.5,
        }
    }
}

impl SemanticSandbox {
    pub fn new(llm: Arc<dyn LlmClient>) -> Self {
        Self {
            llm,
            policies: vec![
                SafetyPolicy {
                    name: "harm-prevention".into(),
                    description: "Reject content that could cause direct harm: code execution without review, data destruction, unauthorized access".into(),
                    severity: 0.9,
                },
                SafetyPolicy {
                    name: "integrity".into(),
                    description: "Reject content that contradicts established facts or fabricates information".into(),
                    severity: 0.7,
                },
                SafetyPolicy {
                    name: "value-alignment".into(),
                    description: "Flag content that may violate core values or ethical guidelines".into(),
                    severity: 0.5,
                },
            ],
        }
    }

    pub fn with_policies(mut self, policies: Vec<SafetyPolicy>) -> Self {
        self.policies = policies;
        self
    }

    /// 审查一个待写入的条目。
    pub async fn review(&self, entry: &ContextEntry) -> Result<SandboxVerdict> {
        let content = format!(
            "L0: {}\nL1: {}",
            entry.l0_abstract,
            entry.l1_overview.as_deref().unwrap_or("")
        );

        let policies_text: String = self
            .policies
            .iter()
            .map(|p| format!("- {} (severity={}): {}", p.name, p.severity, p.description))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"You are a content safety auditor. Review this entry against the following policies.

Policies:
{policies_text}

Entry to review:
{content}

Return a JSON object with:
- "verdict": "pass", "reject", or "quarantine"
- "reason": specific explanation
- "violated_rule": which policy was violated (empty if pass)
- "risk_score": 0.0-1.0
"#
        );

        let opts = LlmOpts {
            max_tokens: Some(512),
            temperature: Some(0.0),
            ..Default::default()
        };

        let response = match self.llm.complete(&prompt, &opts).await {
            Ok(r) => r,
            Err(_) => return Ok(SandboxVerdict::Pass), // LLM 不可用时放行
        };

        #[derive(serde::Deserialize)]
        struct RawVerdict {
            verdict: String,
            reason: String,
            violated_rule: String,
            risk_score: f32,
        }

        let raw: RawVerdict = serde_json::from_str(&response).unwrap_or(RawVerdict {
            verdict: "pass".into(),
            reason: "unable to parse".into(),
            violated_rule: String::new(),
            risk_score: 0.0,
        });

        match raw.verdict.as_str() {
            "reject" => Ok(SandboxVerdict::Reject {
                reason: raw.reason,
                rule: raw.violated_rule,
            }),
            "quarantine" => Ok(SandboxVerdict::Quarantine {
                reason: raw.reason,
                risk_score: raw.risk_score,
            }),
            _ => Ok(SandboxVerdict::Pass),
        }
    }
}

/// 写入闸门 —— 组合关键词检查 + LLM 语义审查。
pub struct WriteGate {
    keyword_check: crate::CharacterConstraint,
    sandbox: SemanticSandbox,
    /// 是否启用 LLM 审查
    llm_review_enabled: bool,
}

impl WriteGate {
    pub fn new(
        keyword_check: crate::CharacterConstraint,
        sandbox: SemanticSandbox,
    ) -> Self {
        Self { keyword_check, sandbox, llm_review_enabled: true }
    }

    pub fn without_llm_review(mut self) -> Self {
        self.llm_review_enabled = false;
        self
    }

    /// 完整审查流程：关键词 → LLM 语义 → 放行/拒绝/隔离。
    pub async fn gate(
        &self,
        entry: &ContextEntry,
    ) -> std::result::Result<SandboxVerdict, crate::ConstraintViolation> {
        // 第一层：关键词快速拦截
        self.keyword_check.check_write(entry)?;

        // 第二层：LLM 语义审查
        if self.llm_review_enabled {
            match self.sandbox.review(entry).await {
                Ok(verdict) => Ok(verdict),
                Err(_) => Ok(SandboxVerdict::Pass), // LLM 不可用 → 放行
            }
        } else {
            Ok(SandboxVerdict::Pass)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharacterConstraint, CoreValue};
    use agent_context_db_core::{ContextUri, TenantId};

    #[tokio::test]
    async fn write_gate_keyword_check_blocks() {
        let cc = CharacterConstraint::new(vec![CoreValue {
            name: "honesty".into(),
            forbidden_terms: vec!["fabricate".into()],
        }]);
        let sandbox = SemanticSandbox::new(Arc::new(crate::MockLlmClient));
        let gate = WriteGate::new(cc, sandbox).without_llm_review();

        let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();
        let entry = ContextEntry::new_text(uri, TenantId(uuid::Uuid::nil()), "we fabricate results");

        let result = gate.gate(&entry).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_gate_passes_safe_content() {
        let cc = CharacterConstraint::new(vec![CoreValue {
            name: "honesty".into(),
            forbidden_terms: vec!["fabricate".into()],
        }]);
        let sandbox = SemanticSandbox::new(Arc::new(crate::MockLlmClient));
        let gate = WriteGate::new(cc, sandbox).without_llm_review();

        let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();
        let entry = ContextEntry::new_text(uri, TenantId(uuid::Uuid::nil()), "we found and fixed the bug");

        let result = gate.gate(&entry).await;
        assert!(result.is_ok());
    }
}
