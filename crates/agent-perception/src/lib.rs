//! # agent-perception
//!
//! 感知域 —— 输入解析 + PII 检测与可逆加密 + ContextDescriptor 构建。
//!
//! 作为 visual_script NodeDefinition 注册：`"perception.observe"`（Impure + Async）

mod context;
mod pii;

pub use context::ContextDescriptor;
pub use pii::{PiiScanner, PiiStrategy};

use agent_types_core::{Action};
use async_trait::async_trait;

/// 感知器 —— 将原始输入转换为结构化上下文描述
#[async_trait]
pub trait Perceiver: Send + Sync {
    async fn perceive(&self, raw_input: &str) -> ContextDescriptor;
}

/// 感知管道 —— 解析链：RawInput → Parsed → PII scan → ContextDescriptor
pub struct PerceptionPipeline {
    pii_scanner: Option<PiiScanner>,
}

impl PerceptionPipeline {
    pub fn new() -> Self {
        Self { pii_scanner: None }
    }

    pub fn with_pii(mut self, scanner: PiiScanner) -> Self {
        self.pii_scanner = Some(scanner);
        self
    }

    /// 执行完整感知管道
    pub async fn run(&self, raw_input: &str) -> ContextDescriptor {
        let mut ctx = ContextDescriptor::from_raw(raw_input);
        if let Some(ref scanner) = self.pii_scanner {
            scanner.scan_and_mask(&mut ctx).await;
        }
        ctx
    }
}

impl Default for PerceptionPipeline {
    fn default() -> Self {
        Self::new()
    }
}
