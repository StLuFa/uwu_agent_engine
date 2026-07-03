//! 双向链接与反向引用（#2）。

use crate::block::BlockId;
use crate::doc::DocId;
use serde::{Deserialize, Serialize};

/// 引用目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkTarget {
    Doc(DocId),
    Block(DocId, BlockId),
    /// 悬空引用（目标不存在），由 Lint 修复。
    Broken(String),
}

impl LinkTarget {
    /// 目标的稳定字符串键（供 backlinks 索引）。
    pub fn key(&self) -> String {
        match self {
            LinkTarget::Doc(d) => format!("doc:{d}"),
            LinkTarget::Block(d, b) => format!("block:{d}:{b}"),
            LinkTarget::Broken(s) => format!("broken:{s}"),
        }
    }
}

/// 页面内联引用（解析自 `[[target]]` 语法或显式 mention）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiLink {
    /// 引用发起 Block。
    pub from: BlockId,
    /// 引用目标。
    pub to: LinkTarget,
    /// 显示文本。
    pub anchor_text: String,
}

/// 引用图 —— 正向 + 反向双索引。由 wiki-core 维护，持久化走 [`crate::storage::LinkStore`]。
pub trait LinkGraph: Send + Sync {
    /// 本 Block/Doc 引用了谁（正向）。
    fn outbound(&self, from: &BlockId) -> Vec<WikiLink>;
    /// 谁引用了本 Doc/Block（反向，即 backlinks）。
    fn backlinks(&self, target: &LinkTarget) -> Vec<WikiLink>;
    /// 全库悬空引用（供 Lint 审计）。
    fn broken_links(&self) -> Vec<WikiLink>;
}

/// 从文本中解析 `[[target]]` 内联链接（骨架：仅提取语法，不解析目标存在性）。
pub fn parse_links(from: &BlockId, text: &str) -> Vec<WikiLink> {
    let mut links = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            if let Some(end) = text[i + 2..].find("]]") {
                let inner = &text[i + 2..i + 2 + end];
                links.push(WikiLink {
                    from: from.clone(),
                    to: LinkTarget::Broken(inner.to_string()),
                    anchor_text: inner.to_string(),
                });
                i = i + 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_inline_links() {
        let from = BlockId::new();
        let links = parse_links(&from, "see [[Rust Async]] and [[Tokio]] pages");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].anchor_text, "Rust Async");
        assert_eq!(links[1].anchor_text, "Tokio");
    }

    #[test]
    fn parse_no_links() {
        let from = BlockId::new();
        assert!(parse_links(&from, "plain text, no links").is_empty());
    }
}
