//! LLM 客户端端口（M0 抽象；具体 provider 由宿主注入）。
//!
//! context-db 的语义处理（L0/L1 生成、去重、意图分析）依赖此端口，
//! 但核心不绑定任何具体 LLM 引擎。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("provider: {0}")]
    Provider(String),
    #[error("timeout")]
    Timeout,
    #[error("rate limited")]
    RateLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmOpts {
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl Default for LlmOpts {
    fn default() -> Self {
        Self {
            model: None,
            max_tokens: Some(1024),
            temperature: Some(0.2),
        }
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 文本补全。
    async fn complete(&self, prompt: &str, opts: &LlmOpts) -> Result<String, LlmError>;
    /// 生成 embedding。
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError>;
}
