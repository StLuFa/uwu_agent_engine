//! Character 写入约束（M3）：核心价值观作为 write 前置钩子。
//!
//! 支持两层校验：
//! - 快速路径：关键词匹配（`forbidden_terms`）
//! - 语义路径：LLM 语义审查（注入 `LlmClient` 后启用）

use agent_context_db_core::{ContextEntry, LlmClient, LlmOpts};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("character constraint violated: {0}")]
pub struct ConstraintViolation(pub String);

/// 单条核心价值观（从 uwu://.../character/core_values.md 加载）。
#[derive(Debug, Clone)]
pub struct CoreValue {
    pub name: String,
    /// 价值描述（用于 LLM 语义审查）。
    pub description: String,
    /// 禁止出现的关键词（快速路径）。
    pub forbidden_terms: Vec<String>,
}

impl CoreValue {
    /// 快速路径：关键词匹配检查。
    fn check_keywords(&self, content: &str) -> std::result::Result<(), ConstraintViolation> {
        let lc = content.to_lowercase();
        for t in &self.forbidden_terms {
            if lc.contains(&t.to_lowercase()) {
                return Err(ConstraintViolation(format!(
                    "value `{}` forbids term `{}`",
                    self.name, t
                )));
            }
        }
        Ok(())
    }
}

/// ContextStore.write 前置校验钩子。
///
/// 构造时注入可选 `LlmClient`：有则走"关键词 → LLM 语义"双重审查，
/// 无则仅关键词快速路径。
pub struct CharacterConstraint {
    core_values: Vec<CoreValue>,
    llm: Option<Arc<dyn LlmClient>>,
}

impl CharacterConstraint {
    /// 仅关键词检查（无 LLM 语义审查）。
    pub fn new(core_values: Vec<CoreValue>) -> Self {
        Self {
            core_values,
            llm: None,
        }
    }

    /// 带 LLM 语义审查的完整约束。
    pub fn with_llm(core_values: Vec<CoreValue>, llm: Arc<dyn LlmClient>) -> Self {
        Self {
            core_values,
            llm: Some(llm),
        }
    }

    /// write 前置钩子：对 entry 内容逐条价值观校验。
    ///
    /// 两条检查路径：
    /// 1. 关键词快速路径（始终执行）
    /// 2. LLM 语义审查（有 `LlmClient` 时执行）
    pub async fn check_write(
        &self,
        entry: &ContextEntry,
    ) -> std::result::Result<(), ConstraintViolation> {
        // 构建待审查的完整文本
        let full_text = build_full_text(entry);

        // 1. 关键词快速路径
        for v in &self.core_values {
            v.check_keywords(&entry.l0_abstract)?;
            if let Some(ov) = &entry.l1_overview {
                v.check_keywords(ov)?;
            }
        }

        // 2. LLM 语义审查
        if let Some(llm) = &self.llm {
            let values_desc: Vec<String> = self
                .core_values
                .iter()
                .map(|v| format!("- {}: {}", v.name, v.description))
                .collect();
            let values_text = values_desc.join("\n");

            let prompt = format!(
                r#"You are a character constraint auditor for an AI agent.

The agent's core values are:
{values_text}

Review the following context entry content for semantic violations of ANY of these values.
A violation means the content goes against a core value in meaning, intent, or implication
(even if it doesn't use exact forbidden words).

Content:
{full_text}

Return a JSON object:
If no violations: {{"violation": false}}
If violation found: {{"violation": true, "value_name": "<which value>", "reason": "<brief explanation>"}}

Respond with ONLY the JSON object.
"#
            );

            let opts = LlmOpts {
                max_tokens: Some(200),
                temperature: Some(0.0),
                ..Default::default()
            };

            match llm.complete(&prompt, &opts).await {
                Ok(response) => {
                    #[derive(serde::Deserialize)]
                    struct SemanticResult {
                        violation: bool,
                        #[serde(default)]
                        value_name: String,
                        #[serde(default)]
                        reason: String,
                    }

                    let json_str = extract_json_object(&response);
                    if let Ok(result) = serde_json::from_str::<SemanticResult>(&json_str) {
                        if result.violation {
                            return Err(ConstraintViolation(format!(
                                "semantic violation of `{}`: {}",
                                result.value_name, result.reason
                            )));
                        }
                    }
                    // 若 LLM 响应解析失败，放行（安全侧不阻塞写操作）
                }
                Err(_) => {
                    // LLM 调用失败时放行，避免阻塞正常写入
                }
            }
        }

        Ok(())
    }

    /// 同步关键词检查（不涉及 LLM）。
    pub fn check_write_sync(
        &self,
        entry: &ContextEntry,
    ) -> std::result::Result<(), ConstraintViolation> {
        for v in &self.core_values {
            v.check_keywords(&entry.l0_abstract)?;
            if let Some(ov) = &entry.l1_overview {
                v.check_keywords(ov)?;
            }
        }
        Ok(())
    }
}

/// 构建待审查的完整文本（L0 + L1）。
fn build_full_text(entry: &ContextEntry) -> String {
    let mut text = entry.l0_abstract.clone();
    if let Some(ov) = &entry.l1_overview {
        text.push_str("\n---\n");
        text.push_str(ov);
    }
    text
}

/// 从 LLM 响应中提取 JSON 对象。
fn extract_json_object(text: &str) -> String {
    let text = text.trim();
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_context_db_core::{ContextUri, TenantId};
    use uuid::Uuid;

    #[test]
    fn forbidden_term_blocks_write() {
        let cc = CharacterConstraint::new(vec![CoreValue {
            name: "honesty".into(),
            description: "Always tell the truth and do not fabricate information.".into(),
            forbidden_terms: vec!["fabricate".into()],
        }]);
        let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();
        let entry = ContextEntry::new_text(uri, TenantId(Uuid::nil()), "we fabricate results");
        assert!(cc.check_write_sync(&entry).is_err());
    }

    #[test]
    fn allowed_content_passes_keyword_check() {
        let cc = CharacterConstraint::new(vec![CoreValue {
            name: "honesty".into(),
            description: "Always tell the truth.".into(),
            forbidden_terms: vec!["fabricate".into(), "deceive".into()],
        }]);
        let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();
        let entry = ContextEntry::new_text(uri, TenantId(Uuid::nil()), "we verified the results");
        assert!(cc.check_write_sync(&entry).is_ok());
    }

    #[test]
    fn builds_full_text_with_overview() {
        let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();
        let mut entry = ContextEntry::new_text(uri, TenantId(Uuid::nil()), "abstract");
        entry.l1_overview = Some("overview content".into());
        let text = build_full_text(&entry);
        assert!(text.contains("abstract"));
        assert!(text.contains("overview content"));
    }
}
