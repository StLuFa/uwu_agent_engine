//! # agent-memory
//!
//! 统一记忆 —— 四型记忆视图 + 向量相似检索 + Episode 巩固。
//!
//! 一个向量嵌入空间 + 一个元数据索引。四型记忆（Episodic/Semantic/Procedural/Working）
//! 是同一存储上的查询视图。
//!
//! 作为 visual_script NodeDefinition 注册：`"memory.retrieve"`（Impure + Async）

mod consolidate;
mod embedding;
mod retrieve;
mod types;
mod unified;
#[cfg(feature = "visual-script")]
pub mod vs_nodes;

pub use consolidate::{Episode, consolidate_episode};
pub use embedding::Embedding;
pub use retrieve::RetrievalIntent;
pub use types::{Memory, MemoryScore, MemoryType};
pub use unified::UnifiedMemory;

/// 记忆检索结果
#[derive(Debug, Clone)]
pub struct RetrievedMemories {
    pub items: Vec<Memory>,
    pub total_score: f32,
}

impl RetrievedMemories {
    pub fn new(items: Vec<Memory>) -> Self {
        let total = if items.is_empty() {
            0.0
        } else {
            items.iter().map(|m| m.score.total).sum::<f32>() / items.len() as f32
        };
        Self { items, total_score: total }
    }
}

/// 记忆门面 —— 封装 UnifiedMemory + 便捷方法
pub struct MemoryFacade {
    memory: UnifiedMemory,
}

impl MemoryFacade {
    pub fn new(embedding_dim: usize) -> Self {
        Self {
            memory: UnifiedMemory::new(embedding_dim),
        }
    }

    /// 使用外部向量存储后端（需 `database` feature）。
    ///
    /// ```ignore
    /// use uwu_database::vector::memory::MemoryVectorStore;
    /// let facade = MemoryFacade::new(256)
    ///     .with_vector_store(MemoryVectorStore::new());
    /// ```
    #[cfg(feature = "database")]
    pub fn with_vector_store(
        mut self,
        store: Box<dyn uwu_database::vector::VectorStore>,
    ) -> Self {
        self.memory = self.memory.with_vector_store(store);
        self
    }

    /// 检索记忆（返回 RetrievalIntent 默认策略的结果）
    pub fn retrieve(&mut self, query: impl Into<String>) -> RetrievedMemories {
        let intent = RetrievalIntent::simple(query);
        let items = self.memory.retrieve(&intent);
        RetrievedMemories::new(items)
    }

    /// 使用向量存储检索（需 `database` feature），适合大规模记忆库。
    #[cfg(feature = "database")]
    pub async fn retrieve_async(
        &self,
        query: impl Into<String>,
        embedding_dim: usize,
        top_k: usize,
    ) -> Result<RetrievedMemories, String> {
        let q = query.into();
        let emb = Embedding::mock(&q, embedding_dim);
        let items = self
            .memory
            .retrieve_with_vector_store(&emb.values, top_k)
            .await?;
        Ok(RetrievedMemories::new(items))
    }

    /// 将当前所有记忆同步到向量存储（需 `database` feature）。
    #[cfg(feature = "database")]
    pub async fn sync_to_vector_store(&self) -> Result<(), String> {
        self.memory.sync_to_vector_store().await
    }

    /// 持久化 State 快照
    pub fn persist_state(
        &mut self,
        agent_id: impl Into<String>,
        snapshot_json: impl Into<String>,
    ) {
        self.memory.persist_state(agent_id, snapshot_json);
    }

    /// 巩固 Episode
    pub fn consolidate(&mut self, episode: &Episode) {
        self.memory.consolidate_episode(episode);
    }

    /// 获取底层 UnifiedMemory
    pub fn inner(&self) -> &UnifiedMemory {
        &self.memory
    }

    /// 可修改的底层 UnifiedMemory
    pub fn inner_mut(&mut self) -> &mut UnifiedMemory {
        &mut self.memory
    }
}

impl Default for MemoryFacade {
    fn default() -> Self {
        Self::new(16)
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_facade_retrieve_works() {
        let mut facade = MemoryFacade::new(16);
        let emb = Embedding::mock("rust programming guide", 16).values;
        facade.inner_mut().upsert(Memory::new(
            MemoryType::Semantic,
            "rust programming guide",
            emb,
        ));

        let result = facade.retrieve("rust");
        assert!(!result.items.is_empty());
    }

    #[test]
    fn memory_facade_persist_and_retrieve() {
        let mut facade = MemoryFacade::new(16);
        facade.persist_state("agent-1", r#"{"version":1}"#);

        let result = facade.retrieve("state");
        assert!(!result.items.is_empty());
    }

    #[test]
    fn retrieved_memories_empty() {
        let result = RetrievedMemories::new(vec![]);
        assert!(result.items.is_empty());
        assert!((result.total_score - 0.0).abs() < 0.001);
    }

    // ---- database feature tests ----

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn memory_facade_with_vector_store() {
        use uwu_database::vector::memory::MemoryVectorStore;

        let store = MemoryVectorStore::new();
        let mut facade = MemoryFacade::new(256)
            .with_vector_store(Box::new(store));

        // Insert memories
        let emb = Embedding::mock("rust async programming guide", 256).values;
        facade.inner_mut().upsert(Memory::new(
            MemoryType::Semantic,
            "rust async programming guide",
            emb.clone(),
        ));
        facade.inner_mut().upsert(Memory::new(
            MemoryType::Semantic,
            "python data science handbook",
            Embedding::mock("python data science handbook", 256).values,
        ));

        // Sync to vector store
        assert!(facade.sync_to_vector_store().await.is_ok());

        // Retrieve via vector store
        let result = facade
            .retrieve_async("rust programming", 256, 5)
            .await
            .unwrap();
        assert!(!result.items.is_empty());
        assert!(result.items[0].content.contains("rust"));
    }

    #[cfg(feature = "database")]
    #[test]
    fn memory_facade_has_vector_store_flag() {
        use uwu_database::vector::memory::MemoryVectorStore;

        let store = MemoryVectorStore::new();
        let facade = MemoryFacade::new(64)
            .with_vector_store(Box::new(store));
        assert!(facade.inner().has_vector_store());
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn memory_facade_sync_empty_is_ok() {
        use uwu_database::vector::memory::MemoryVectorStore;

        let store = MemoryVectorStore::new();
        let facade = MemoryFacade::new(64)
            .with_vector_store(Box::new(store));
        // Syncing with no memories should succeed.
        assert!(facade.sync_to_vector_store().await.is_ok());
    }
}
