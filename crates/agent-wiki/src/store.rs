//! MemoryWikiStore —— 内存中的 Wiki 存储实现

use crate::page::{PageStatus, WikiPage};
use crate::repo::{WikiRepo, WikiRepoError};
use async_trait::async_trait;
use std::collections::HashMap;

/// 内存中的 Wiki 存储
///
/// 开发调试用。生产环境应接 uwu_database 的 VectorStore + PostgreSQL。
pub struct MemoryWikiStore {
    pages: HashMap<String, WikiPage>,
}

impl MemoryWikiStore {
    pub fn new() -> Self {
        Self {
            pages: HashMap::new(),
        }
    }

    /// 当前页面数量
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

impl Default for MemoryWikiStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WikiRepo for MemoryWikiStore {
    async fn save(&mut self, page: &WikiPage) -> Result<(), WikiRepoError> {
        // 检查标题重复
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
        Ok(self
            .pages
            .values()
            .find(|p| p.title == title)
            .cloned())
    }

    async fn search(&self, query: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
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
        // 标题匹配优先
        results.sort_by(|a, b| {
            let a_title = a.title.to_lowercase().contains(&lower);
            let b_title = b.title.to_lowercase().contains(&lower);
            b_title.cmp(&a_title)
        });
        Ok(results)
    }

    async fn by_tag(&self, tag: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        Ok(self
            .pages
            .values()
            .filter(|p| p.tags.iter().any(|t| t == tag))
            .cloned()
            .collect())
    }

    async fn by_category(&self, category: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        Ok(self
            .pages
            .values()
            .filter(|p| p.category == category)
            .cloned()
            .collect())
    }

    async fn by_status(&self, status: PageStatus) -> Result<Vec<WikiPage>, WikiRepoError> {
        Ok(self
            .pages
            .values()
            .filter(|p| p.status == status)
            .cloned()
            .collect())
    }

    async fn delete(&mut self, page_id: &str) -> Result<bool, WikiRepoError> {
        Ok(self.pages.remove(page_id).is_some())
    }

    async fn count(&self) -> Result<usize, WikiRepoError> {
        Ok(self.pages.len())
    }

    async fn list(&self, offset: usize, limit: usize) -> Result<Vec<WikiPage>, WikiRepoError> {
        let mut pages: Vec<_> = self
            .pages
            .values()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect();
        pages.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(pages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_page(_id: &str, title: &str) -> WikiPage {
        WikiPage::new(title, "content", "docs", "agent-1")
    }

    #[tokio::test]
    async fn save_and_get() {
        let mut store = MemoryWikiStore::new();
        let page = sample_page("p1", "Hello Wiki");
        store.save(&page).await.unwrap();

        let got = store.get(&page.page_id).await.unwrap().unwrap();
        assert_eq!(got.title, "Hello Wiki");
    }

    #[tokio::test]
    async fn search_by_content() {
        let mut store = MemoryWikiStore::new();
        store
            .save(&sample_page("p1", "Rust Guide"))
            .await
            .unwrap();
        store
            .save(&sample_page("p2", "Python Tutorial"))
            .await
            .unwrap();

        let results = store.search("rust").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Guide");
    }

    #[tokio::test]
    async fn filter_by_tag() {
        let mut store = MemoryWikiStore::new();
        let mut page = sample_page("p1", "Page");
        page.add_tag("rust");
        store.save(&page).await.unwrap();

        let results = store.by_tag("rust").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn duplicate_title_rejected() {
        let mut store = MemoryWikiStore::new();
        let p1 = sample_page("p1", "Same Title");
        let p2 = sample_page("p2", "Same Title");
        store.save(&p1).await.unwrap();
        let err = store.save(&p2).await.unwrap_err();
        assert!(matches!(err, WikiRepoError::DuplicateTitle(_)));
    }

    #[tokio::test]
    async fn delete_removes_page() {
        let mut store = MemoryWikiStore::new();
        let page = sample_page("p1", "T");
        store.save(&page).await.unwrap();
        assert!(store.delete(&page.page_id).await.unwrap());
        assert!(store.get(&page.page_id).await.unwrap().is_none());
    }
}
