//! # wiki-workflow
//!
//! LLM Wiki 工作流骨架：Ingest（原料→知识）/ Query（问答→反写）/ Lint（审计）。
//! LLM 是 wiki 的全职编辑 —— 知识随时间复利增长。

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wiki_core::Result;
use wiki_llm::{LlmCapability, QaAnswer};

// ===========================================================================
// 反写策略
// ===========================================================================

/// Query 答案反写策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriteBackPolicy {
    /// 从不反写。
    Never,
    /// 置信度超阈值自动反写。
    Auto { confidence_threshold: f32 },
    /// 先询问再反写。
    AskFirst,
}

impl Default for WriteBackPolicy {
    fn default() -> Self {
        WriteBackPolicy::AskFirst
    }
}

// ===========================================================================
// Ingest
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngestSource {
    Text(String),
    DocId(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IngestResult {
    pub touched_docs: Vec<String>,
    pub created_docs: Vec<String>,
    pub contradictions: Vec<String>,
}

// ===========================================================================
// Lint
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LintReport {
    pub duplicates: Vec<String>,
    pub broken_links: Vec<String>,
    pub missing_pages: Vec<String>,
    pub stale_pages: Vec<String>,
}

// ===========================================================================
// WikiDomain —— 三管线统一入口
// ===========================================================================

/// 工作流域。依赖注入的 [`LlmCapability`] 端口，不 `use` 任何 LLM 引擎具体类型。
pub struct WikiDomain {
    llm: Arc<dyn LlmCapability>,
}

impl WikiDomain {
    pub fn new(llm: Arc<dyn LlmCapability>) -> Self {
        Self { llm }
    }

    /// 消化新原料进 wiki（骨架）。
    pub async fn ingest(&self, source: IngestSource) -> Result<IngestResult> {
        let text = match source {
            IngestSource::Text(t) => t,
            IngestSource::DocId(_) => String::new(),
        };
        // 骨架：真实实现会抽取实体、对比现有知识、更新/创建页面。
        let _summary = self
            .llm
            .summarize(&[wiki_llm::TextUnit {
                id: "ingest".into(),
                text,
                path: vec![],
            }])
            .await?;
        Ok(IngestResult::default())
    }

    /// 问答（含可选反写）。
    pub async fn query(&self, question: &str, policy: WriteBackPolicy) -> Result<QaAnswer> {
        let answer = self.llm.qa(question, None).await?;
        // 骨架：根据 policy 决定是否把答案反写回 wiki。
        let _ = policy;
        Ok(answer)
    }

    /// 触发审计（骨架）。
    pub async fn lint(&self) -> Result<LintReport> {
        Ok(LintReport::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use wiki_llm::TextUnit;

    struct StubLlm;

    #[async_trait]
    impl LlmCapability for StubLlm {
        async fn embed(&self, units: &[TextUnit]) -> Result<Vec<Vec<f32>>> {
            Ok(units.iter().map(|_| vec![0.0]).collect())
        }
        async fn search(&self, _q: &str, _k: usize) -> Result<Vec<(TextUnit, f32)>> {
            Ok(vec![])
        }
        async fn complete(&self, _u: &TextUnit, _p: &str) -> Result<String> {
            Ok(String::new())
        }
        async fn qa(&self, question: &str, _scope: Option<&str>) -> Result<QaAnswer> {
            Ok(QaAnswer {
                answer: format!("answer to: {question}"),
                citations: vec![],
            })
        }
        async fn summarize(&self, _u: &[TextUnit]) -> Result<String> {
            Ok("summary".into())
        }
    }

    #[tokio::test]
    async fn query_returns_answer() {
        let domain = WikiDomain::new(Arc::new(StubLlm));
        let ans = domain.query("what is rust?", WriteBackPolicy::Never).await.unwrap();
        assert!(ans.answer.contains("what is rust?"));
    }

    #[tokio::test]
    async fn ingest_and_lint_skeleton() {
        let domain = WikiDomain::new(Arc::new(StubLlm));
        let r = domain.ingest(IngestSource::Text("hello".into())).await.unwrap();
        assert!(r.touched_docs.is_empty());
        let report = domain.lint().await.unwrap();
        assert!(report.duplicates.is_empty());
    }
}
