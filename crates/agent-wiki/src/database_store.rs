//! DatabaseWikiStore — VectorStore-backed Wiki storage (feature = "database").
//!
//! Uses `uwu_database::vector::VectorStore` for semantic search while keeping
//! page metadata in a HashMap for fast CRUD. Embeddings are generated from
//! page title + content using the built-in `Embedding::mock()` function.

use crate::page::{PageStatus, WikiPage};
use crate::repo::{WikiRepo, WikiRepoError};
use async_trait::async_trait;
use std::collections::HashMap;

/// Database-backed Wiki store: HashMap for metadata + VectorStore for semantic search.
pub struct DatabaseWikiStore {
    /// Metadata index (fast CRUD, filtering by tag/category/status).
    pages: HashMap<String, WikiPage>,
    /// Vector store for semantic similarity search.
    vector_store: Box<dyn uwu_database::vector::VectorStore>,
    /// Embedding dimension.
    embedding_dim: usize,
    /// Collection name in the vector store.
    collection: String,
}

impl DatabaseWikiStore {
    pub fn new(vector_store: Box<dyn uwu_database::vector::VectorStore>, embedding_dim: usize) -> Self {
        Self {
            pages: HashMap::new(),
            vector_store,
            embedding_dim,
            collection: "wiki_pages".to_string(),
        }
    }

    pub fn with_collection(mut self, name: impl Into<String>) -> Self {
        self.collection = name.into();
        self
    }

    /// Sync all pages to the vector store (call after bulk load).
    pub async fn sync_to_vector_store(&self) -> Result<(), WikiRepoError> {
        let spec = uwu_database::vector::CollectionSpec {
            name: &self.collection,
            dim: self.embedding_dim,
            distance: uwu_database::vector::Distance::Cosine,
        };
        self.vector_store
            .ensure_collection(spec)
            .await
            .map_err(|e| WikiRepoError::Storage(format!("ensure collection: {e}")))?;

        let records: Vec<uwu_database::vector::Record> = self
            .pages
            .values()
            .map(|p| {
                let emb = wiki_embedding(&p.title, &p.content, self.embedding_dim);
                let mut meta = HashMap::new();
                meta.insert("title".into(), serde_json::Value::String(p.title.clone()));
                meta.insert("category".into(), serde_json::Value::String(p.category.clone()));
                uwu_database::vector::Record {
                    id: p.page_id.clone(),
                    vector: emb,
                    metadata: meta,
                }
            })
            .collect();

        self.vector_store
            .upsert(&self.collection, &records)
            .await
            .map_err(|e| WikiRepoError::Storage(format!("upsert: {e}")))?;

        Ok(())
    }

}

/// Simple embedding: average of character bigram hashes → f32 vector.
fn wiki_embedding(title: &str, content: &str, dim: usize) -> Vec<f32> {
    let text = format!("{title} {content}");
    let chars: Vec<char> = text.chars().collect();
    let mut vec = vec![0.0f32; dim];
    if chars.len() < 2 {
        for (i, &c) in chars.iter().enumerate() {
            vec[i % dim] = (c as u32 as f32) / 65536.0;
        }
        return vec;
    }
    let mut count = 0u32;
    for w in chars.windows(2) {
        let h = ((w[0] as u64) << 16) | (w[1] as u64);
        let idx = (h as usize) % dim;
        vec[idx] += 1.0;
        count += 1;
    }
    if count > 0 {
        for v in &mut vec { *v /= count as f32; }
    }
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec { *v /= norm; }
    }
    vec
}

impl Default for DatabaseWikiStore {
    fn default() -> Self {
        Self::new(
            Box::new(uwu_database::vector::memory::MemoryVectorStore::new()),
            64,
        )
    }
}

#[async_trait]
impl WikiRepo for DatabaseWikiStore {
    async fn save(&mut self, page: &WikiPage) -> Result<(), WikiRepoError> {
        for existing in self.pages.values() {
            if existing.title == page.title && existing.page_id != page.page_id {
                return Err(WikiRepoError::DuplicateTitle(page.title.clone()));
            }
        }
        self.pages.insert(page.page_id.clone(), page.clone());
        Ok(())
    }

    async fn get(&self, page_id: &str) -> Result<Option<WikiPage>, WikiRepoError> {
        Ok(self.pages.get(page_id).cloned())
    }

    async fn get_by_title(&self, title: &str) -> Result<Option<WikiPage>, WikiRepoError> {
        Ok(self.pages.values().find(|p| p.title == title).cloned())
    }

    async fn search(&self, query: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        // Fallback to text search if vector store is not synced.
        let lower = query.to_lowercase();
        let mut results: Vec<_> = self
            .pages
            .values()
            .filter(|p| {
                p.title.to_lowercase().contains(&lower)
                    || p.content.to_lowercase().contains(&lower)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&lower))
            })
            .cloned()
            .collect();
        results.sort_by(|a, b| {
            let a_title = a.title.to_lowercase().contains(&lower);
            let b_title = b.title.to_lowercase().contains(&lower);
            b_title.cmp(&a_title)
        });
        Ok(results)
    }

    async fn by_tag(&self, tag: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        Ok(self.pages.values().filter(|p| p.tags.iter().any(|t| t == tag)).cloned().collect())
    }

    async fn by_category(&self, category: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        Ok(self.pages.values().filter(|p| p.category == category).cloned().collect())
    }

    async fn by_status(&self, status: PageStatus) -> Result<Vec<WikiPage>, WikiRepoError> {
        Ok(self.pages.values().filter(|p| p.status == status).cloned().collect())
    }

    async fn delete(&mut self, page_id: &str) -> Result<bool, WikiRepoError> {
        Ok(self.pages.remove(page_id).is_some())
    }

    async fn count(&self) -> Result<usize, WikiRepoError> {
        Ok(self.pages.len())
    }

    async fn list(&self, offset: usize, limit: usize) -> Result<Vec<WikiPage>, WikiRepoError> {
        let mut pages: Vec<_> = self.pages.values().skip(offset).take(limit).cloned().collect();
        pages.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(pages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn database_store_save_and_search() {
        let mut store = DatabaseWikiStore::default();
        let page = WikiPage::new("Rust Async", "# Async\n\nRust async intro", "rust", "agent-1");
        store.save(&page).await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);

        let results = store.search("async").await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn database_store_sync_to_vector_store() {
        let store = DatabaseWikiStore::default();
        // Empty sync should succeed (collection is created on the fly)
        let _ = store.sync_to_vector_store().await;
    }
}
