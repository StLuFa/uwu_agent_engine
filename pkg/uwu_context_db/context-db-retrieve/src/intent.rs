//! 意图分析器：将自然语言查询拆为 0-N 个类型化查询。
//!
//! - [`RuleBasedIntentAnalyzer`]：关键词匹配版，不依赖 LLM，用于快速跑通 M1 链路。
//! - 生产级 `LlmIntentAnalyzer`（LLM 驱动）在 `LlmClient` 实现后对接。

use agent_context_db_core::{ContextUri, MemoryClass, Result};
use async_trait::async_trait;

use crate::{IntentAnalyzer, QueryKind, RetrieveContext, TypedQuery};

/// 基于关键词规则的意图分析器（零外部依赖，先行跑通 M1 管线）。
pub struct RuleBasedIntentAnalyzer {
    /// 默认用户 ID（当 ctx 未指定时使用）。
    default_user_id: String,
    /// 默认 Agent ID。
    default_agent_id: String,
}

impl RuleBasedIntentAnalyzer {
    pub fn new(default_user_id: impl Into<String>, default_agent_id: impl Into<String>) -> Self {
        Self {
            default_user_id: default_user_id.into(),
            default_agent_id: default_agent_id.into(),
        }
    }

    /// 根据关键词推断 QueryKind 与 target_dirs。
    fn classify(&self, query: &str, ctx: &RetrieveContext) -> Vec<TypedQuery> {
        let lower = query.to_lowercase();
        let user_id = ctx.user_id.as_deref().unwrap_or(&self.default_user_id);
        let agent_id = ctx.agent_id.as_deref().unwrap_or(&self.default_agent_id);

        let mut results = Vec::new();

        // ── EventRecall：事件/时间相关 ──
        if contains_any(&lower, &["when", "happened", "event", "那天", "之前", "上次"]) {
            results.push(TypedQuery {
                kind: QueryKind::EventRecall,
                text: query.to_string(),
                target_dirs: vec![
                    memories_dir(user_id, agent_id, "events"),
                    memories_dir(user_id, agent_id, "cases"),
                ],
                expected_class: Some(MemoryClass::Events),
            });
        }

        // ── EntityLookup：人/项目/实体查询 ──
        if contains_any(&lower, &["who", "what is", "entity", "project", "是谁", "什么是", "哪个"]) {
            results.push(TypedQuery {
                kind: QueryKind::EntityLookup,
                text: query.to_string(),
                target_dirs: vec![
                    memories_dir(user_id, agent_id, "entities"),
                    memories_dir(user_id, agent_id, "profile"),
                ],
                expected_class: Some(MemoryClass::Entities),
            });
        }

        // ── SkillReuse：操作/方法 ──
        if contains_any(&lower, &["how to", "how do", "步骤", "方法", "怎么", "如何", "教程"]) {
            results.push(TypedQuery {
                kind: QueryKind::SkillReuse,
                text: query.to_string(),
                target_dirs: vec![
                    memories_dir(user_id, agent_id, "skills"),
                    memories_dir(user_id, agent_id, "tools"),
                    uri(format!("uwu://{}/agent/{}/experiences", user_id, agent_id)),
                ],
                expected_class: Some(MemoryClass::Skills),
            });
        }

        // ── PatternMatch：模式/模板 ──
        if contains_any(&lower, &["pattern", "template", "模式", "模板", "惯例", "typically"]) {
            results.push(TypedQuery {
                kind: QueryKind::PatternMatch,
                text: query.to_string(),
                target_dirs: vec![
                    memories_dir(user_id, agent_id, "patterns"),
                    memories_dir(user_id, agent_id, "cases"),
                ],
                expected_class: Some(MemoryClass::Patterns),
            });
        }

        // ── StateSnapshot：状态 ──
        if contains_any(&lower, &["state", "snapshot", "状态", "当前", "now", "recently", "最近"]) {
            results.push(TypedQuery {
                kind: QueryKind::StateSnapshot,
                text: query.to_string(),
                target_dirs: vec![
                    uri(format!("uwu://{}/agent/{}/state/short", user_id, agent_id)),
                    uri(format!("uwu://{}/agent/{}/state/mid", user_id, agent_id)),
                ],
                expected_class: None,
            });
        }

        // ── PersonaRelation：关系 ──
        if contains_any(&lower, &["relation", "persona", "关系", "朋友", "信任", "trust"]) {
            results.push(TypedQuery {
                kind: QueryKind::PersonaRelation,
                text: query.to_string(),
                target_dirs: vec![
                    uri(format!("uwu://{}/agent/{}/persona/relations", user_id, agent_id)),
                ],
                expected_class: None,
            });
        }

        // ── Default: SemanticSearch（preferences + 全局）──
        if results.is_empty() || contains_any(&lower, &["prefer", "like", "dislike", "喜欢", "偏好", "remember", "记得"]) {
            results.push(TypedQuery {
                kind: QueryKind::SemanticSearch,
                text: query.to_string(),
                target_dirs: vec![
                    memories_dir(user_id, agent_id, "preferences"),
                    memories_dir(user_id, agent_id, "profile"),
                    memories_dir(user_id, agent_id, "cases"),
                    memories_dir(user_id, agent_id, "events"),
                    memories_dir(user_id, agent_id, "skills"),
                    memories_dir(user_id, agent_id, "tools"),
                ],
                expected_class: None,
            });
        }

        results
    }
}

#[async_trait]
impl IntentAnalyzer for RuleBasedIntentAnalyzer {
    async fn analyze(&self, query: &str, ctx: &RetrieveContext) -> Result<Vec<TypedQuery>> {
        Ok(self.classify(query, ctx))
    }
}

// ── helpers ──

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn uri(s: impl Into<String>) -> ContextUri {
    ContextUri(s.into())
}

fn memories_dir(tenant: &str, agent_id: &str, sub: &str) -> ContextUri {
    uri(format!("uwu://{}/agent/{}/memories/{}", tenant, agent_id, sub))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RetrieveContext {
        RetrieveContext {
            user_id: Some("u1".into()),
            agent_id: Some("a1".into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn event_query_is_classified() {
        let ia = RuleBasedIntentAnalyzer::new("u1", "a1");
        let tqs = ia.analyze("what happened last week?", &ctx()).await.unwrap();
        assert_eq!(tqs[0].kind, QueryKind::EventRecall);
        assert_eq!(tqs[0].expected_class, Some(MemoryClass::Events));
    }

    #[tokio::test]
    async fn howto_query_targets_skills() {
        let ia = RuleBasedIntentAnalyzer::new("u1", "a1");
        let tqs = ia.analyze("how to deploy the app?", &ctx()).await.unwrap();
        assert_eq!(tqs[0].kind, QueryKind::SkillReuse);
        assert!(!tqs[0].target_dirs.is_empty());
    }

    #[tokio::test]
    async fn ambiguous_query_gets_semantic_search() {
        let ia = RuleBasedIntentAnalyzer::new("u1", "a1");
        let tqs = ia.analyze("rust async patterns", &ctx()).await.unwrap();
        // "pattern" 触发 PatternMatch，但也有默认 fallback
        assert!(tqs.iter().any(|t| t.kind == QueryKind::PatternMatch));
    }

    #[tokio::test]
    async fn preference_query_falls_back_to_semantic() {
        let ia = RuleBasedIntentAnalyzer::new("u1", "a1");
        let tqs = ia.analyze("what does the user like?", &ctx()).await.unwrap();
        // "like" → SemanticSearch
        assert_eq!(tqs[0].kind, QueryKind::SemanticSearch);
    }
}
