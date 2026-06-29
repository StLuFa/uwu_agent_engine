//! WikiPage —— 结构化知识页面 + MVCC 版本化

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Wiki 页面状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageStatus {
    /// 草稿
    Draft,
    /// 已发布
    Published,
    /// 已归档
    Archived,
}

/// Wiki 页面单次版本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageVersion {
    /// 版本号（单调递增）
    pub version: u64,
    /// 此版本的标题
    pub title: String,
    /// 此版本的内容（Markdown）
    pub content: String,
    /// 编辑摘要
    pub edit_summary: String,
    /// 编辑者 Agent ID
    pub edited_by: String,
    /// 编辑时间
    pub edited_at: DateTime<Utc>,
}

/// Wiki 页面 —— 结构化知识条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPage {
    /// 页面唯一 ID
    pub page_id: String,
    /// 当前标题（最新版本）
    pub title: String,
    /// 当前内容（最新版本，Markdown）
    pub content: String,
    /// 标签列表
    pub tags: Vec<String>,
    /// 分类
    pub category: String,
    /// 页面状态
    pub status: PageStatus,
    /// 当前版本号
    pub current_version: u64,
    /// 完整版本历史
    pub version_history: Vec<WikiPageVersion>,
    /// 创建者 Agent ID
    pub created_by: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后修改时间
    pub updated_at: DateTime<Utc>,
    /// 引用其他页面的 ID 列表
    pub references: Vec<String>,
    /// 被哪些页面引用
    pub referenced_by: Vec<String>,
}

impl WikiPage {
    /// 创建新页面（草稿状态，版本 0）
    pub fn new(
        title: impl Into<String>,
        content: impl Into<String>,
        category: impl Into<String>,
        created_by: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let title = title.into();
        let content = content.into();
        let created_by = created_by.into();

        let initial_version = WikiPageVersion {
            version: 0,
            title: title.clone(),
            content: content.clone(),
            edit_summary: "initial creation".into(),
            edited_by: created_by.clone(),
            edited_at: now,
        };

        Self {
            page_id: uuid::Uuid::new_v4().to_string(),
            title,
            content,
            tags: Vec::new(),
            category: category.into(),
            status: PageStatus::Draft,
            current_version: 0,
            version_history: vec![initial_version],
            created_by,
            created_at: now,
            updated_at: now,
            references: Vec::new(),
            referenced_by: Vec::new(),
        }
    }

    /// 编辑页面 —— 追加新版本
    pub fn edit(
        &mut self,
        new_title: impl Into<String>,
        new_content: impl Into<String>,
        edit_summary: impl Into<String>,
        edited_by: impl Into<String>,
    ) -> u64 {
        let now = Utc::now();
        let new_title = new_title.into();
        let new_content = new_content.into();

        self.current_version += 1;
        self.title = new_title.clone();
        self.content = new_content.clone();
        self.updated_at = now;

        self.version_history.push(WikiPageVersion {
            version: self.current_version,
            title: new_title,
            content: new_content,
            edit_summary: edit_summary.into(),
            edited_by: edited_by.into(),
            edited_at: now,
        });

        self.current_version
    }

    /// 回滚到指定版本
    pub fn rollback_to(&mut self, version: u64, edited_by: impl Into<String>) -> Option<u64> {
        let target = self.version_history.iter().find(|v| v.version == version)?;
        let title = target.title.clone();
        let content = target.content.clone();
        self.edit(
            title,
            content,
            format!("rollback to version {version}"),
            edited_by,
        );
        Some(self.current_version)
    }

    /// 发布页面
    pub fn publish(&mut self) {
        self.status = PageStatus::Published;
    }

    /// 归档页面
    pub fn archive(&mut self) {
        self.status = PageStatus::Archived;
    }

    /// 添加标签
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// 添加引用
    pub fn add_reference(&mut self, page_id: impl Into<String>) {
        let pid = page_id.into();
        if !self.references.contains(&pid) {
            self.references.push(pid);
        }
    }

    /// 获取指定版本的快照
    pub fn version_at(&self, version: u64) -> Option<&WikiPageVersion> {
        self.version_history.iter().find(|v| v.version == version)
    }

    /// 比较两个版本的差异（简易文本 diff）
    pub fn diff_versions(&self, v1: u64, v2: u64) -> Option<PageDiff> {
        let ver1 = self.version_at(v1)?;
        let ver2 = self.version_at(v2)?;
        Some(PageDiff {
            title_changed: ver1.title != ver2.title,
            content_added: ver2.content.len().saturating_sub(ver1.content.len()),
            content_removed: ver1.content.len().saturating_sub(ver2.content.len()),
            v1,
            v2,
        })
    }
}

/// 版本间差异摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDiff {
    pub title_changed: bool,
    pub content_added: usize,
    pub content_removed: usize,
    pub v1: u64,
    pub v2: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_page_is_draft() {
        let page = WikiPage::new("Title", "Content", "docs", "agent-1");
        assert_eq!(page.status, PageStatus::Draft);
        assert_eq!(page.current_version, 0);
        assert_eq!(page.version_history.len(), 1);
    }

    #[test]
    fn edit_increments_version() {
        let mut page = WikiPage::new("Title", "Content", "docs", "agent-1");
        let v = page.edit("New Title", "New Content", "updated", "agent-2");
        assert_eq!(v, 1);
        assert_eq!(page.current_version, 1);
        assert_eq!(page.title, "New Title");
        assert_eq!(page.version_history.len(), 2);
    }

    #[test]
    fn rollback_creates_new_version() {
        let mut page = WikiPage::new("V0", "C0", "docs", "agent-1");
        page.edit("V1", "C1", "edit", "agent-1");
        page.edit("V2", "C2", "edit", "agent-1");

        let v = page.rollback_to(0, "agent-1").unwrap();
        assert_eq!(v, 3); // new version created
        assert_eq!(page.title, "V0");
        assert_eq!(page.version_history.len(), 4);
    }

    #[test]
    fn publish_and_archive() {
        let mut page = WikiPage::new("T", "C", "docs", "a");
        page.publish();
        assert_eq!(page.status, PageStatus::Published);
        page.archive();
        assert_eq!(page.status, PageStatus::Archived);
    }

    #[test]
    fn diff_versions_works() {
        let mut page = WikiPage::new("T", "short", "docs", "a");
        page.edit("T", "much longer content", "expanded", "a");

        let diff = page.diff_versions(0, 1).unwrap();
        assert!(!diff.title_changed);
        assert!(diff.content_added > 0);
    }
}
