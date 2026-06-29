//! # agent-wiki
//!
//! Agent Wiki —— 多 Agent 协作的结构化知识库。
//!
//! ## 核心概念
//!
//! - **WikiPage** — 结构化知识条目，含 MVCC 版本历史
//! - **WikiRepo** — 可插拔存储后端（MemoryStore / PostgreSQL + 向量检索）
//! - **Multi-Agent 协作** — 多 Agent 通过 CRDT 无冲突编辑同一页面
//!
//! ## 架构定位
//!
//! ```text
//! Agent Wiki 操作流:
//!   创建页面:  Perception → WikiRepo.save()
//!   编辑页面:  fork(State) → 沙盒推演 → evaluate() → WikiRepo.save()
//!   版本历史:  WikiPage.version_history → diff_versions()
//!   语义搜索:  WikiRepo.search(query) → 向量检索（接 uwu_database）
//!   协作编辑:  agent-collaboration.delegate() + CRDT merge
//!   变更通知:  agent-mesh.publish(TOPIC_WIKI_UPDATED)
//!   安全审计:  GuardLayer(指令/参数/能力/预算/egress)
//! ```

pub mod page;
pub mod repo;
pub mod store;

pub use page::{PageDiff, PageStatus, WikiPage, WikiPageVersion};
pub use repo::{WikiRepo, WikiRepoError};
pub use store::MemoryWikiStore;

// ===========================================================================
// 单元测试（集成场景）
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_lifecycle() {
        let mut store = MemoryWikiStore::new();

        // 1. 创建页面
        let mut page = WikiPage::new("Rust Async", "# Async\n\nRust async intro", "rust", "agent-1");
        page.add_tag("rust");
        page.add_tag("async");
        page.publish();
        store.save(&page).await.unwrap();

        // 2. 搜索
        let results = store.search("async").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, PageStatus::Published);

        // 3. 编辑 → 新版本
        let mut found = store.get(&page.page_id).await.unwrap().unwrap();
        found.edit(
            "Rust Async Deep Dive",
            "# Async\n\nDetailed async guide\n\n## Tokio",
            "expanded content",
            "agent-2",
        );
        found.add_reference("related-page-id");
        store.save(&found).await.unwrap();

        // 4. 版本历史
        let found2 = store.get(&page.page_id).await.unwrap().unwrap();
        assert_eq!(found2.current_version, 1);
        assert_eq!(found2.version_history.len(), 2);

        // 5. 版本差异
        let diff = found2.diff_versions(0, 1).unwrap();
        assert!(diff.content_added > 0);

        // 6. 按分类筛选
        let by_cat = store.by_category("rust").await.unwrap();
        assert_eq!(by_cat.len(), 1);

        // 7. 列表分页
        let list = store.list(0, 10).await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn multi_page_search() {
        let mut store = MemoryWikiStore::new();
        for i in 0..5 {
            let page = WikiPage::new(
                format!("Page {i}"),
                format!("Content of page {i}"),
                "general",
                "agent-1",
            );
            store.save(&page).await.unwrap();
        }
        assert_eq!(store.count().await.unwrap(), 5);

        let results = store.search("Page 3").await.unwrap();
        assert_eq!(results.len(), 1);
    }
}
