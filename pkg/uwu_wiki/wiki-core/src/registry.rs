//! Block 类型注册表 + 渲染 trait。

use crate::block::{Block, BlockType};
use std::collections::HashMap;

/// 自定义 Block 类型注册表。核心不硬编码具体类型。
#[derive(Default)]
pub struct BlockTypeRegistry {
    custom: HashMap<String, CustomBlockSpec>,
}

/// 自定义类型规格（骨架）。
pub struct CustomBlockSpec {
    pub name: String,
    /// 校验内容是否合法。
    pub validate: fn(&serde_json::Value) -> bool,
}

impl BlockTypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: CustomBlockSpec) {
        self.custom.insert(spec.name.clone(), spec);
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.custom.contains_key(name)
    }

    /// 校验一个 Block 的内容对其类型是否合法。
    pub fn validate(&self, block: &Block) -> bool {
        match &block.ty {
            BlockType::Custom(name) => self
                .custom
                .get(name)
                .map(|spec| (spec.validate)(&block.content.0))
                .unwrap_or(false),
            // 内置类型骨架阶段一律通过。
            _ => true,
        }
    }
}

/// Block → 目标格式渲染 trait。
pub trait Render {
    fn render_markdown(&self, block: &Block) -> String;
}

/// 默认 Markdown 渲染器（骨架实现）。
pub struct MarkdownRenderer;

impl Render for MarkdownRenderer {
    fn render_markdown(&self, block: &Block) -> String {
        let text = block.content.as_plain_text();
        match &block.ty {
            BlockType::Heading => format!("# {text}"),
            BlockType::Quote => format!("> {text}"),
            BlockType::Code => format!("```\n{text}\n```"),
            BlockType::Divider => "---".to_string(),
            _ => text,
        }
    }
}
