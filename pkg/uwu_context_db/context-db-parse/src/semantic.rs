//! `SemanticProcessorImpl`：L0/L1 生成 + 自底向上聚合。
//!
//! 使用 `LlmClient` 为条目生成摘要（L0 ~100 tokens）和概览（L1 ~2k tokens）。

use agent_context_db_core::{ContextUri, LlmClient, LlmOpts, Result};
use async_trait::async_trait;
use std::sync::Arc;

use crate::SemanticProcessor;

/// 基于 `LlmClient` 的语义处理器实现。
pub struct SemanticProcessorImpl {
    llm: Arc<dyn LlmClient>,
}

impl SemanticProcessorImpl {
    pub fn new(llm: Arc<dyn LlmClient>) -> Self {
        Self { llm }
    }
}

#[async_trait]
impl SemanticProcessor for SemanticProcessorImpl {
    async fn generate_abstract(&self, uri: &ContextUri) -> Result<String> {
        let prompt = format!(
            r#"You are a context summarizer. Write a concise L0 abstract (~100 tokens) for:
URI: {uri}

An abstract should capture: what this entry is about, its category, and key information.
Respond with ONLY the abstract text, no additional commentary.
"#
        );

        let opts = LlmOpts {
            max_tokens: Some(150),
            temperature: Some(0.1),
            ..Default::default()
        };

        self.llm.complete(&prompt, &opts).await
            .map(|s| s.trim().to_string())
            .map_err(|e| agent_context_db_core::ContextError::Storage(
                format!("llm generate_abstract: {e}")
            ))
    }

    async fn generate_overview(&self, uri: &ContextUri) -> Result<String> {
        let prompt = format!(
            r#"You are a context organizer. Write an L1 overview (~1000 tokens) for:
URI: {uri}

An overview should include:
1. A structured table of contents with sections
2. Key concepts and their relationships
3. Navigation hints for related entries

Format as Markdown with ## section headers.
Respond with ONLY the overview text.
"#
        );

        let opts = LlmOpts {
            max_tokens: Some(1500),
            temperature: Some(0.2),
            ..Default::default()
        };

        self.llm.complete(&prompt, &opts).await
            .map(|s| s.trim().to_string())
            .map_err(|e| agent_context_db_core::ContextError::Storage(
                format!("llm generate_overview: {e}")
            ))
    }

    async fn aggregate_upward(&self, root: &ContextUri) -> Result<()> {
        // 自底向上聚合：将子条目的 L0 合并为父目录的 L1
        // 实际实现需要遍历子项、收集摘要、调用 LLM 合成
        // 骨架实现留待完整管线
        let _ = root;
        Ok(())
    }

    async fn multimodal_to_text(&self, uri: &ContextUri) -> Result<(String, String)> {
        // 多模态转文本：image/audio → text description
        // 需要多模态 LLM 能力，骨架留待 MCP 对接
        Err(agent_context_db_core::ContextError::Unsupported(
            format!("multimodal_to_text not yet implemented for {uri}")
        ))
    }
}
