//! # agent-perception
//!
//! 感知域 —— 输入解析 + PII 检测与可逆加密 + ContextDescriptor 构建。
//!
//! 作为 visual_script NodeDefinition 注册：`"perception.observe"`（Impure + Async）

mod context;
mod pii;
#[cfg(feature = "visual-script")]
pub mod vs_nodes;

pub use context::{ContextDescriptor, ParsedInput};
pub use pii::{PiiScanner, PiiStrategy};

use async_trait::async_trait;

/// 感知器 —— 将原始输入转换为结构化上下文描述
#[async_trait]
pub trait Perceiver: Send + Sync {
    /// 从原始输入生成上下文描述
    async fn perceive(&self, raw_input: &str) -> ContextDescriptor;
}

/// 感知管道 —— 解析链：RawInput → Parse → PII scan → ContextDescriptor
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

    /// 执行完整感知管道：parse → PII scan → ContextDescriptor
    pub async fn run(&self, raw_input: &str) -> ContextDescriptor {
        let mut ctx = ContextDescriptor::from_raw(raw_input);
        if let Some(ref scanner) = self.pii_scanner {
            scanner.scan_and_mask(&mut ctx.description);
        }
        ctx
    }

    /// 先解析结构化输入，再走 PII → ContextDescriptor
    pub async fn run_parsed(&self, parsed: &ParsedInput) -> ContextDescriptor {
        let mut ctx = ContextDescriptor::new(&parsed.text);
        ctx.raw_data = serde_json::to_value(&parsed.fields).unwrap_or_default();
        if let Some(ref scanner) = self.pii_scanner {
            scanner.scan_and_mask(&mut ctx.description);
        }
        ctx
    }

    /// 是否有 PII 扫描器
    pub fn has_pii(&self) -> bool {
        self.pii_scanner.is_some()
    }
}

impl Default for PerceptionPipeline {
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

    #[test]
    fn pipeline_default_no_pii() {
        let pipeline = PerceptionPipeline::new();
        assert!(!pipeline.has_pii());
    }

    #[test]
    fn pipeline_with_pii() {
        let pipeline =
            PerceptionPipeline::new().with_pii(PiiScanner::new(PiiStrategy::Mask));
        assert!(pipeline.has_pii());
    }

    #[tokio::test]
    async fn pipeline_masks_pii_in_context() {
        let pipeline =
            PerceptionPipeline::new().with_pii(PiiScanner::new(PiiStrategy::Mask));
        let ctx = pipeline.run("hello alice@example.com").await;
        assert!(!ctx.description.contains("alice@example.com"));
        assert!(ctx.description.contains("[email]"));
    }

    #[tokio::test]
    async fn pipeline_no_pii_passthrough() {
        let pipeline = PerceptionPipeline::new();
        let ctx = pipeline.run("hello world").await;
        assert_eq!(ctx.description, "hello world");
    }

    #[tokio::test]
    async fn run_parsed_with_fields() {
        let pipeline =
            PerceptionPipeline::new().with_pii(PiiScanner::new(PiiStrategy::Mask));
        let parsed = ParsedInput {
            raw: r#"{"name":"Alice","email":"alice@example.com"}"#.into(),
            text: "Alice alice@example.com".into(),
            fields: vec![("name".into(), "Alice".into())],
            is_structured: true,
        };
        let ctx = pipeline.run_parsed(&parsed).await;
        assert!(!ctx.description.contains("alice@example.com"));
    }

    #[tokio::test]
    async fn parsed_input_from_text() {
        let parsed = ParsedInput::from_text("hello world");
        assert!(!parsed.is_structured);
        assert_eq!(parsed.text, "hello world");
    }

    #[tokio::test]
    async fn parsed_input_from_json() {
        let parsed = ParsedInput::from_json(
            r#"{"key":"val"}"#,
            vec![("key".into(), "val".into())],
        );
        assert!(parsed.is_structured);
        assert_eq!(parsed.fields.len(), 1);
    }
}
