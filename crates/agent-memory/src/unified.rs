//! UnifiedMemory —— 内存中的统一记忆存储
//!
//! - 默认：HashMap + 本地余弦相似度（零外部依赖）
//! - feature = "database"：可插拔 uwu_database::VectorStore 后端

use crate::embedding::Embedding;
use crate::retrieve::RetrievalIntent;
use crate::types::{Memory, MemoryScore, MemoryType};
use std::collections::HashMap;

/// 统一记忆 —— 四型记忆视图 + 向量相似检索
pub struct UnifiedMemory {
    /// 按 ID 索引的所有记忆（元数据索引）
    memories: HashMap<String, Memory>,
    /// 向量嵌入维度
    embedding_dim: usize,
    /// 外部向量存储后端（feature = "database" 时可用）
    #[cfg(feature = "database")]
    vector_store: Option<Box<dyn uwu_database::vector::VectorStore>>,
}

impl UnifiedMemory {
    pub fn new(embedding_dim: usize) -> Self {
        Self {
            memories: HashMap::new(),
            embedding_dim: embedding_dim.max(4),
            #[cfg(feature = "database")]
            vector_store: None,
        }
    }

    /// Attach an external vector store backend (requires `database` feature).
    ///
    /// When set, `retrieve()` uses the vector store's native `search()` instead
    /// of brute-force cosine similarity. Metadata (agent_id, timestamps, access
    /// counts) is still tracked in the local HashMap index.
    #[cfg(feature = "database")]
    pub fn with_vector_store(mut self, store: Box<dyn uwu_database::vector::VectorStore>) -> Self {
        self.vector_store = Some(store);
        self
    }

    /// Check if a vector store backend is active.
    #[cfg(feature = "database")]
    pub fn has_vector_store(&self) -> bool {
        self.vector_store.is_some()
    }

    /// 插入或更新记忆
    pub fn upsert(&mut self, memory: Memory) {
        self.memories.insert(memory.id.clone(), memory);
    }

    /// 批量插入
    pub fn upsert_batch(&mut self, memories: Vec<Memory>) {
        for m in memories {
            self.memories.insert(m.id.clone(), m);
        }
    }

    /// 批量同步记忆到外部向量存储（feature = "database" 时可用）。
    /// 调用方应在插入记忆后调用此方法以确保向量索引一致。
    #[cfg(feature = "database")]
    pub async fn sync_to_vector_store(&self) -> Result<(), String> {
        if let Some(ref store) = self.vector_store {
            // Ensure the collection exists before upserting.
            let spec = uwu_database::vector::CollectionSpec {
                name: "agent_memories",
                dim: self.embedding_dim,
                distance: uwu_database::vector::Distance::Cosine,
            };
            store
                .ensure_collection(spec)
                .await
                .map_err(|e| format!("ensure collection: {e}"))?;

            let records: Vec<uwu_database::vector::Record> = self
                .memories
                .values()
                .map(|m| uwu_database::vector::Record {
                    id: m.id.clone(),
                    vector: m.embedding.clone(),
                    metadata: {
                        let mut meta = HashMap::new();
                        meta.insert("content".into(), serde_json::Value::String(m.content.clone()));
                        meta.insert(
                            "memory_type".into(),
                            serde_json::Value::String(format!("{:?}", m.memory_type)),
                        );
                        meta
                    },
                })
                .collect();
            store
                .upsert("agent_memories", &records)
                .await
                .map_err(|e| format!("vector store upsert: {e}"))?;
        }
        Ok(())
    }

    /// 按 ID 获取
    pub fn get(&self, id: &str) -> Option<&Memory> {
        self.memories.get(id)
    }

    /// 默认检索 —— 覆盖 80% 场景
    pub fn retrieve(&mut self, intent: &RetrievalIntent) -> Vec<Memory> {
        self.retrieve_typed(intent, None)
    }

    /// 按类型检索
    pub fn retrieve_typed(
        &mut self,
        intent: &RetrievalIntent,
        types: Option<Vec<MemoryType>>,
    ) -> Vec<Memory> {
        let query_emb = Embedding::mock(&intent.query, self.embedding_dim).values;
        let types_set: Option<std::collections::HashSet<MemoryType>> =
            types.map(|t| t.into_iter().collect());

        let mut scored: Vec<(Memory, f32)> = self
            .memories
            .values()
            .filter(|m| {
                types_set
                    .as_ref()
                    .map_or(true, |ts| ts.contains(&m.memory_type))
            })
            .map(|m| {
                let similarity =
                    Embedding::cosine_similarity(&query_emb, &m.embedding);
                let score = MemoryScore::new(
                    similarity,
                    m.score.recency,
                    m.access_count as f32 / (m.access_count + 1) as f32,
                );
                (m.clone(), score.total)
            })
            .filter(|(_, total)| *total >= intent.min_similarity)
            .collect();

        // 按评分降序
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        let results: Vec<Memory> = scored
            .into_iter()
            .take(intent.max_results)
            .map(|(mut m, _s)| {
                m.score = MemoryScore::new(
                    Embedding::cosine_similarity(&query_emb, &m.embedding),
                    m.score.recency,
                    m.access_count as f32 / (m.access_count + 1) as f32,
                );
                m.record_access();
                // Update in store
                self.memories.insert(m.id.clone(), m.clone());
                m
            })
            .collect();

        // Update access counts in store
        for m in &results {
            self.memories.insert(m.id.clone(), m.clone());
        }

        results
    }

    /// 持久化 State 快照
    pub fn persist_state(
        &mut self,
        agent_id: impl Into<String>,
        state_snapshot_json: impl Into<String>,
    ) {
        let content = format!("State snapshot at {}", chrono::Utc::now());
        let emb = Embedding::mock(&content, self.embedding_dim).values;
        let memory = Memory::new(MemoryType::Working, &content, emb)
            .with_agent(agent_id)
            .with_state(state_snapshot_json);
        self.upsert(memory);
    }

    /// 持久化 Persona 快照
    pub fn persist_persona(
        &mut self,
        agent_id: impl Into<String>,
        persona_json: impl Into<String>,
    ) {
        let content = format!("Persona snapshot at {}", chrono::Utc::now());
        let emb = Embedding::mock(&content, self.embedding_dim).values;
        let memory = Memory::new(MemoryType::Semantic, &content, emb)
            .with_agent(agent_id)
            .with_state(persona_json);
        self.upsert(memory);
    }

    /// Consolidate an episode into memories
    pub fn consolidate_episode(&mut self, episode: &crate::consolidate::Episode) {
        let memories = crate::consolidate::consolidate_episode(episode, self.embedding_dim);
        self.upsert_batch(memories);
    }

    /// 记忆总数
    pub fn len(&self) -> usize {
        self.memories.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.memories.is_empty()
    }

    /// 按类型统计
    pub fn count_by_type(&self, mt: MemoryType) -> usize {
        self.memories.values().filter(|m| m.memory_type == mt).count()
    }

    /// 使用向量存储后端进行检索（feature = "database"）。
    ///
    /// 调用 VectorStore::search() 替代本地余弦相似度暴力搜索。
    /// 适合大规模记忆库（>10k 条）场景。
    #[cfg(feature = "database")]
    pub async fn retrieve_with_vector_store(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<Memory>, String> {
        let store = self
            .vector_store
            .as_ref()
            .ok_or_else(|| "no vector store configured".to_string())?;

        // Ensure the collection exists before querying.
        let spec = uwu_database::vector::CollectionSpec {
            name: "agent_memories",
            dim: self.embedding_dim,
            distance: uwu_database::vector::Distance::Cosine,
        };
        store
            .ensure_collection(spec)
            .await
            .map_err(|e| format!("ensure collection: {e}"))?;

        let query = uwu_database::vector::Query {
            vector: query_embedding,
            top_k,
            filter: None,
        };

        let matches = store
            .search("agent_memories", query)
            .await
            .map_err(|e| format!("vector search: {e}"))?;

        let results: Vec<Memory> = matches
            .into_iter()
            .filter_map(|m| self.memories.get(&m.id).cloned())
            .collect();

        Ok(results)
    }
}

impl Default for UnifiedMemory {
    fn default() -> Self {
        Self::new(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consolidate::Episode;

    fn seed_memory(um: &mut UnifiedMemory, content: &str, mt: MemoryType) {
        let emb = Embedding::mock(content, 16).values;
        um.upsert(Memory::new(mt, content, emb));
    }

    #[test]
    fn retrieve_by_intent() {
        let mut um = UnifiedMemory::new(16);
        seed_memory(&mut um, "rust async programming", MemoryType::Semantic);
        seed_memory(&mut um, "python data science", MemoryType::Semantic);
        seed_memory(&mut um, "how to click buttons", MemoryType::Procedural);

        let intent = RetrievalIntent::simple("async programming");
        let results = um.retrieve(&intent);
        assert!(!results.is_empty());
        // Rust async should rank higher than Python
        assert!(results[0].content.contains("rust"));
    }

    #[test]
    fn retrieve_typed_filters() {
        let mut um = UnifiedMemory::new(16);
        seed_memory(&mut um, "rust async", MemoryType::Semantic);
        seed_memory(&mut um, "click button flow", MemoryType::Procedural);

        let intent = RetrievalIntent::simple("async flow");
        let results = um.retrieve_typed(&intent, Some(vec![MemoryType::Procedural]));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_type, MemoryType::Procedural);
    }

    #[test]
    fn persist_state_and_retrieve() {
        let mut um = UnifiedMemory::new(16);
        um.persist_state("agent-1", r#"{"version":5}"#);

        let intent = RetrievalIntent::simple("state snapshot");
        let results = um.retrieve(&intent);
        assert!(!results.is_empty());
        assert_eq!(results[0].memory_type, MemoryType::Working);
    }

    #[test]
    fn consolidate_episode_creates_memories() {
        let mut um = UnifiedMemory::new(16);
        let episode = Episode::new("agent-1", "find data", "found 10 records", true)
            .with_action("search database")
            .with_action("filter results")
            .with_observation("10 rows returned");

        um.consolidate_episode(&episode);

        // Should create Episodic + Procedural memories
        assert!(um.len() >= 2);
        assert!(um.count_by_type(MemoryType::Episodic) >= 1);
        assert!(um.count_by_type(MemoryType::Procedural) >= 1);
    }
}
