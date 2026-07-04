//! LLM 客户端实现（M3 对接层）。
//!
//! - [`HttpLlmClient`]：OpenAI 兼容 HTTP API（支持 OpenAI / Anthropic proxy / Ollama / 本地模型）
//! - [`MockLlmClient`]：确定性响应，用于测试 L5 解析层管线
//!
//! 两个实现均满足 core 的 `LlmClient` trait，由 composition root 注入。

use agent_context_db_core::{LlmClient, LlmError, LlmOpts, LlmStream};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ===========================================================================
// HttpLlmClient
// ===========================================================================

/// OpenAI 兼容 HTTP LLM 客户端。
///
/// 支持任意兼容 `/v1/chat/completions` 和 `/v1/embeddings` 的后端。
pub struct HttpLlmClient {
    client: Client,
    api_base: String,
    api_key: String,
    default_model: String,
}

impl HttpLlmClient {
    /// `api_base` 示例：`https://api.openai.com/v1`
    pub fn new(api_base: impl Into<String>, api_key: impl Into<String>, default_model: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("reqwest Client::build");
        Self {
            client,
            api_base: api_base.into(),
            api_key: api_key.into(),
            default_model: default_model.into(),
        }
    }

    fn model<'a>(&'a self, opts: &'a LlmOpts) -> &'a str {
        opts.model.as_deref().unwrap_or(&self.default_model)
    }
}

#[async_trait]
impl LlmClient for HttpLlmClient {
    async fn complete(&self, prompt: &str, opts: &LlmOpts) -> Result<String, LlmError> {
        let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));
        let model = self.model(opts);
        let max_tokens = opts.max_tokens.unwrap_or(1024);
        let temperature = opts.temperature.unwrap_or(0.2);

        let body = ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            max_tokens,
            temperature,
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Provider(format!("http request: {e}"))
                }
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!("HTTP {status}: {text}")));
        }

        let body: ChatCompletionResponse = resp.json().await.map_err(|e| {
            LlmError::Provider(format!("json decode: {e}"))
        })?;

        body.choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| LlmError::Provider("empty response".into()))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let url = format!("{}/embeddings", self.api_base.trim_end_matches('/'));
        let model = &self.default_model;

        let body = EmbeddingRequest {
            model: model.to_string(),
            input: text.to_string(),
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Provider(format!("http request: {e}"))
                }
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Provider(format!("HTTP {status}: {text}")));
        }

        let body: EmbeddingResponse = resp.json().await.map_err(|e| {
            LlmError::Provider(format!("json decode: {e}"))
        })?;

        body.data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| LlmError::Provider("empty embedding response".into()))
    }

    async fn complete_json(
        &self, prompt: &str, schema: &agent_context_db_core::JsonSchema, opts: &LlmOpts,
    ) -> Result<String, LlmError> {
        let full = format!("{prompt}\n\nReturn ONLY valid JSON. Schema: {}", schema.schema);
        self.complete(&full, opts).await
    }

    /// 流式生成 — 收集完整响应后缓冲返回。
    async fn stream_complete(
        &self, prompt: &str, opts: &LlmOpts,
    ) -> Result<Box<dyn LlmStream + Send>, LlmError> {
        let text = self.complete(prompt, opts).await?;
        Ok(Box::new(SseStream { text, pos: 0 }))
    }

    /// 批量补全 — 并行发出多个 HTTP 请求。
    async fn batch_complete(
        &self, prompts: &[String], opts: &LlmOpts,
    ) -> Result<Vec<String>, LlmError> {
        let handles: Vec<_> = prompts.iter().map(|p| {
            let client = self.client.clone();
            let url = format!("{}/chat/completions", self.api_base.trim_end_matches('/'));
            let model = self.model(opts).to_string();
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": p}],
                "max_tokens": opts.max_tokens.unwrap_or(1024),
                "temperature": opts.temperature.unwrap_or(0.2),
            });
            let key = self.api_key.clone();
            async move {
                client.post(&url)
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send().await
            }
        }).collect();

        let mut results = Vec::new();
        for h in handles {
            let resp = h.await.map_err(|e| LlmError::Provider(format!("batch: {e}")))?;
            if resp.status().is_success() {
                let body: ChatCompletionResponse = resp.json().await.unwrap_or(ChatCompletionResponse { choices: vec![] });
                results.push(body.choices.first().map(|c| c.message.content.clone()).unwrap_or_default());
            } else {
                results.push(String::new());
            }
        }
        Ok(results)
    }

    // speculative_complete uses default impl (falls back to complete)
}

struct SseStream {
    text: String,
    pos: usize,
}

impl LlmStream for SseStream {
    fn next_chunk(&mut self) -> Option<std::result::Result<String, agent_context_db_core::LlmError>> {
        if self.pos >= self.text.len() { None }
        else {
            let chunk = self.text[self.pos..].to_string();
            self.pos = self.text.len();
            Some(Ok(chunk))
        }
    }
}

// ===========================================================================
// MockLlmClient
// ===========================================================================

/// 确定性 Mock LLM 客户端，用于测试 L4-L6 管线。
///
/// - `complete()` 根据关键词返回固定响应
/// - `embed()` 返回简单的确定性向量
pub struct MockLlmClient;

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, prompt: &str, _opts: &LlmOpts) -> Result<String, LlmError> {
        let lower = prompt.to_lowercase();

        // 记忆提取
        if lower.contains("extract") && lower.contains("memory") {
            return Ok(r#"[
                {"class": "preferences", "content": "user prefers dark mode", "confidence": 0.9},
                {"class": "cases", "content": "fixed null pointer in parser", "confidence": 0.85}
            ]"#.to_string());
        }

        // 去重决策
        if lower.contains("deduplicate") || lower.contains("merge decision") {
            return Ok(r#"[
                {"action": "merge", "reason": "same topic", "score": 0.92},
                {"action": "create", "reason": "new topic", "score": 0.88}
            ]"#.to_string());
        }

        // 意图分析
        if lower.contains("analyze") && lower.contains("query") {
            return Ok(r#"[
                {"kind": "SemanticSearch", "text": "user preference", "target_dirs": ["uwu://t1/agent/a1/memories/preferences"]}
            ]"#.to_string());
        }

        // L0 摘要
        if lower.contains("summarize") || lower.contains("abstract") {
            return Ok("Generated summary: ".to_string() + &prompt.chars().take(80).collect::<String>());
        }

        // L1 概览
        if lower.contains("overview") || lower.contains("outline") {
            return Ok("## Overview\n\n- Section 1\n- Section 2\n\nGenerated from: ".to_string()
                + &prompt.chars().take(60).collect::<String>());
        }

        // 默认
        Ok(format!(
            "Mock response to: {}",
            &prompt.chars().take(100).collect::<String>()
        ))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let mut vec = vec![0.0f32; 128];
        for (i, b) in text.bytes().enumerate() {
            vec[i % 128] = (b as f32) / 255.0;
        }
        Ok(vec)
    }

    async fn batch_complete(
        &self, prompts: &[String], opts: &LlmOpts,
    ) -> Result<Vec<String>, LlmError> {
        let mut results = Vec::new();
        for p in prompts {
            results.push(self.complete(p, opts).await?);
        }
        Ok(results)
    }

    async fn speculative_complete(
        &self, prompt: &str, opts: &LlmOpts,
    ) -> Result<String, LlmError> {
        self.complete(prompt, opts).await
    }
}

// ===========================================================================
// OpenAI API 类型
// ===========================================================================

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_complete_returns_canned_response() {
        let client = MockLlmClient;
        let result = client
            .complete("extract memory from conversation", &LlmOpts::default())
            .await
            .unwrap();
        assert!(result.contains("preferences"));
        assert!(result.contains("dark mode"));
    }

    #[tokio::test]
    async fn mock_complete_summarize() {
        let client = MockLlmClient;
        let result = client
            .complete("summarize the following text: hello world", &LlmOpts::default())
            .await
            .unwrap();
        assert!(result.starts_with("Generated summary:"));
    }

    #[tokio::test]
    async fn mock_embed_is_deterministic() {
        let client = MockLlmClient;
        let v1 = client.embed("hello").await.unwrap();
        let v2 = client.embed("hello").await.unwrap();
        assert_eq!(v1, v2, "same input should produce same embedding");
    }

    #[tokio::test]
    async fn mock_embed_different_inputs_differ() {
        let client = MockLlmClient;
        let v1 = client.embed("hello").await.unwrap();
        let v2 = client.embed("world").await.unwrap();
        assert_ne!(v1, v2);
    }
}
