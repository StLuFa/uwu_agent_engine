//! WikiRepo —— 知识库持久化 trait

use crate::page::{PageStatus, WikiPage};
use async_trait::async_trait;

/// Wiki 仓库 trait —— 可插拔存储后端
///
/// 当前提供 MemoryStore 实现；后续可接 uwu_database（PostgreSQL+向量检索）。
#[async_trait]
pub trait WikiRepo: Send + Sync {
    /// 保存页面（新建或更新）
    async fn save(&mut self, page: &WikiPage) -> Result<(), WikiRepoError>;

    /// 按 ID 获取页面
    async fn get(&self, page_id: &str) -> Result<Option<WikiPage>, WikiRepoError>;

    /// 按标题精确查找
    async fn get_by_title(&self, title: &str) -> Result<Option<WikiPage>, WikiRepoError>;

    /// 全文搜索（基于内容的简单匹配，后续接向量检索）
    async fn search(&self, query: &str) -> Result<Vec<WikiPage>, WikiRepoError>;

    /// 按标签筛选
    async fn by_tag(&self, tag: &str) -> Result<Vec<WikiPage>, WikiRepoError>;

    /// 按分类筛选
    async fn by_category(&self, category: &str) -> Result<Vec<WikiPage>, WikiRepoError>;

    /// 按状态筛选
    async fn by_status(&self, status: PageStatus) -> Result<Vec<WikiPage>, WikiRepoError>;

    /// 删除页面
    async fn delete(&mut self, page_id: &str) -> Result<bool, WikiRepoError>;

    /// 全部页面数量
    async fn count(&self) -> Result<usize, WikiRepoError>;

    /// 列出所有页面（可分页）
    async fn list(&self, offset: usize, limit: usize) -> Result<Vec<WikiPage>, WikiRepoError>;
}

/// Wiki 仓库错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiRepoError {
    NotFound(String),
    DuplicateTitle(String),
    Storage(String),
}

impl std::fmt::Display for WikiRepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "page not found: {id}"),
            Self::DuplicateTitle(t) => write!(f, "duplicate title: {t}"),
            Self::Storage(msg) => write!(f, "storage error: {msg}"),
        }
    }
}
