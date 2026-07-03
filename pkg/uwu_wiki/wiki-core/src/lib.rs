//! # wiki-core
//!
//! uwu_wiki 核心：Block 引擎 + Document/Op 模型 + 全部存储/LLM 端口 **trait 定义**。
//!
//! ## 设计约束（见 ARCHITECTURE.md §1.5）
//!
//! - **核心纯粹性**：除 serde/uuid/chrono 外零依赖；**不含存储/LLM 实现**。
//! - **端口/适配器**：全部存储能力以 trait（端口）暴露，实现由宿主注入。
//! - **单向依赖**：`wiki-core` 不依赖任何其他 wiki-* crate 或引擎。
//!
//! 参考实现见 `wiki-testkit`（dev-dependency）；生产由 `agent-context-db` 注入。

pub mod block;
pub mod doc;
pub mod error;
pub mod link;
pub mod registry;
pub mod storage;

pub use block::{Block, BlockContent, BlockId, BlockMeta, BlockType};
pub use doc::{DocId, Document, Op, SpaceId};
pub use error::{Result, WikiError};
pub use link::{parse_links, LinkGraph, LinkTarget, WikiLink};
pub use registry::{BlockTypeRegistry, MarkdownRenderer, Render};
pub use storage::{
    BlobId, BlobStore, BlockChange, ChangeKind, DocDiff, DocStore, DocVersionStore, LinkStore,
    MatchMode, OpLog, TextHit, TextIndex, TextQuery, VectorSearchResult, VectorStore, VersionEntry,
    VersionId, WikiStorage,
};

use std::sync::Arc;

/// 知识库空间 —— 注入存储后对外提供读写入口（骨架）。
pub struct WikiSpace {
    pub id: SpaceId,
    storage: Arc<dyn WikiStorage>,
}

impl WikiSpace {
    pub fn new(id: SpaceId, storage: Arc<dyn WikiStorage>) -> Self {
        Self { id, storage }
    }

    pub fn storage(&self) -> &Arc<dyn WikiStorage> {
        &self.storage
    }

    /// 保存文档（走注入的 doc_store）。
    pub async fn save_doc(&self, doc: &Document) -> Result<()> {
        self.storage.doc_store().save(doc).await
    }

    /// 读取文档。
    pub async fn get_doc(&self, id: &DocId) -> Result<Option<Document>> {
        self.storage.doc_store().get(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_version_and_staleness() {
        let mut b = Block::new(BlockType::Paragraph, BlockContent::text("hello"), "agent-1");
        assert!(!b.is_embedding_stale());

        b.embedding = Some(vec![0.1, 0.2]);
        b.embedding_version = b.version;
        assert!(!b.is_embedding_stale());

        b.bump_version();
        assert!(b.is_embedding_stale(), "内容更新后 embedding 应标记陈旧");
    }

    #[test]
    fn document_block_lookup() {
        let root = Block::new(BlockType::Paragraph, BlockContent::text("root"), "a");
        let root_id = root.id.clone();
        let doc = Document::new("Test", root, SpaceId::default());
        assert!(doc.block(&root_id).is_some());
        assert_eq!(doc.root, root_id);
    }
}
