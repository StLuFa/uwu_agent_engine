//! # agent-memory
//!
//! 统一记忆 —— 一个向量 DB（Qdrant）+ 一个元数据 DB（PostgreSQL）。
//! 四型记忆（Episodic / Semantic / Procedural / Working）是查询视图。
//!
//! 作为 visual_script NodeDefinition 注册：`"memory.retrieve"`（Impure + Async）

mod consolidate;
mod embedding;
mod retrieve;
mod types;
mod unified;

pub use consolidate::Episode;
pub use embedding::Embedding;
pub use retrieve::RetrievalIntent;
pub use types::{Memory, MemoryScore, MemoryType};
pub use unified::UnifiedMemory;

use std::sync::Arc;
use uwu_database::{Database, VectorStore};

/// 记忆检索结果
#[derive(Debug, Clone)]
pub struct RetrievedMemories {
    pub items: Vec<Memory>,
    pub total_score: f32,
}

/// 统一记忆门面 —— 组合向量 DB + 元数据 DB
pub struct MemoryFacade {
    db: Database,
}

impl MemoryFacade {
    /// 从已有 Database 构建
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 获取底层数据库引用
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// 获取向量存储（如果已配置）
    pub fn vector_store(&self) -> Option<&Arc<dyn VectorStore>> {
        self.db.vector.as_ref()
    }
}
