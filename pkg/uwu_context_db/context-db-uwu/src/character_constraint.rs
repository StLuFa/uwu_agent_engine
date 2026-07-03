//! Character 写入约束（M3 骨架）：核心价值观作为 write 前置钩子。

use agent_context_db_core::ContextEntry;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("character constraint violated: {0}")]
pub struct ConstraintViolation(pub String);

/// 单条核心价值观（从 uwu://.../character/core_values.md 加载）。
#[derive(Debug, Clone)]
pub struct CoreValue {
    pub name: String,
    /// 禁止出现的关键词（骨架级检查；真实实现用 LLM 语义判断）。
    pub forbidden_terms: Vec<String>,
}

impl CoreValue {
    fn check(&self, content: &str) -> std::result::Result<(), ConstraintViolation> {
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
pub struct CharacterConstraint {
    core_values: Vec<CoreValue>,
}

impl CharacterConstraint {
    pub fn new(core_values: Vec<CoreValue>) -> Self {
        Self { core_values }
    }

    /// write 前置钩子：对 entry 内容逐条价值观校验。
    pub fn check_write(&self, entry: &ContextEntry) -> std::result::Result<(), ConstraintViolation> {
        for v in &self.core_values {
            v.check(&entry.l0_abstract)?;
            if let Some(ov) = &entry.l1_overview {
                v.check(ov)?;
            }
        }
        Ok(())
    }
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
            forbidden_terms: vec!["fabricate".into()],
        }]);
        let uri = ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap();
        let entry = ContextEntry::new_text(uri, TenantId(Uuid::nil()), "we fabricate results");
        assert!(cc.check_write(&entry).is_err());
    }
}
