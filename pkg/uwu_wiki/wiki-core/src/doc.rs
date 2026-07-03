//! Document 模型 + Op 操作枚举。

use crate::block::{Block, BlockId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 文档唯一标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocId(pub String);

impl DocId {
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }
}

impl Default for DocId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 空间（多租户/多知识库隔离单元）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpaceId(pub String);

impl Default for SpaceId {
    fn default() -> Self {
        Self("default".into())
    }
}

/// 结构化文档 —— Block 树。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: DocId,
    pub title: String,
    pub root: BlockId,
    pub version: u64,
    pub space_id: SpaceId,
    pub tags: Vec<String>,
    pub icon: Option<String>,
    pub cover: Option<String>,
    /// 文档内全部 Block（以 `root` 为树根）。
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn new(title: impl Into<String>, root: Block, space_id: SpaceId) -> Self {
        let root_id = root.id.clone();
        Self {
            id: DocId::new(),
            title: title.into(),
            root: root_id,
            version: 0,
            space_id,
            tags: Vec::new(),
            icon: None,
            cover: None,
            blocks: vec![root],
        }
    }

    pub fn block(&self, id: &BlockId) -> Option<&Block> {
        self.blocks.iter().find(|b| &b.id == id)
    }

    pub fn block_mut(&mut self, id: &BlockId) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| &b.id == id)
    }
}

/// 写操作 —— CRDT 合并输入、事件消息体、审计日志三合一。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Op {
    InsertBlock {
        parent: BlockId,
        after: Option<BlockId>,
        block: Block,
    },
    UpdateBlock {
        id: BlockId,
        patch: serde_json::Value,
    },
    DeleteBlock {
        id: BlockId,
    },
    MoveBlock {
        id: BlockId,
        new_parent: BlockId,
        after: Option<BlockId>,
    },
    UpdateDocMeta {
        doc_id: DocId,
        patch: serde_json::Value,
    },
}
