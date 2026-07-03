//! # agent-context-db-version (M2 版本层)
//!
//! 类 Git 的 DAG 版本模型：Commit / Branch / Tag + `VersionStore` 端口。
//!
//! ## 解耦约束
//!
//! - 独立 crate，通过 feature 开关；关闭时 M0/M1 仍可编译。
//! - 依赖 core 的类型（`ContextUri` / `ContentLevel` / `ContentPayload`），
//!   不反向依赖检索层或 uwu 扩展。
//! - `VersionStore` 是端口（零实现）；线性快照 / DAG 后端由宿主注入。

pub mod model;

pub use model::{
    Author, Branch, BranchLifecycle, BranchName, BranchType, ChangeSet, Commit, CommitId,
    CommitMeta, CommitTrigger, ContentHash, ProvenanceLink, ProvenanceRelation, RenameOp,
    SemanticCondition, Tag, TagName, TagType, UriChange, VersionRef,
};

use agent_context_db_core::{ContentLevel, ContentPayload, ContextUri};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("branch exists: {0}")]
    BranchExists(String),
    #[error("merge conflict: {0}")]
    MergeConflict(String),
    #[error("storage: {0}")]
    Storage(String),
}

pub type Result<T> = std::result::Result<T, VersionError>;

/// 时间旅行读取的时间点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AsOfTime {
    Timestamp(chrono::DateTime<chrono::Utc>),
    Commit(CommitId),
}

#[derive(Debug, Clone, Default)]
pub struct LogOpts {
    pub max_count: Option<usize>,
    pub branch: Option<BranchName>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreeDiff {
    pub adds: Vec<ContextUri>,
    pub updates: Vec<ContextUri>,
    pub deletes: Vec<ContextUri>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// 快进（无分叉）。
    FastForward,
    /// 三方合并。
    ThreeWay,
    /// 冲突时优先目标分支。
    Ours,
    /// 冲突时优先来源分支。
    Theirs,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub commit: CommitId,
    pub conflicts: Vec<ContextUri>,
}

/// 版本存储端口（M2）。见 ARCHITECTURE.md §1.6（此为骨架子集，聚焦交付验收面）。
#[async_trait]
pub trait VersionStore: Send + Sync {
    // === 提交 ===
    async fn commit(
        &self,
        scope: &ContextUri,
        changes: ChangeSet,
        meta: CommitMeta,
    ) -> Result<CommitId>;

    // === 分支（含 State fork 沙盒）===
    async fn create_branch(
        &self,
        scope: &ContextUri,
        name: BranchName,
        from: CommitId,
        bt: BranchType,
    ) -> Result<Branch>;
    async fn list_branches(&self, scope: &ContextUri) -> Result<Vec<Branch>>;
    async fn delete_branch(&self, scope: &ContextUri, name: &BranchName) -> Result<()>;

    // === 标签 ===
    async fn create_tag(&self, scope: &ContextUri, tag: Tag) -> Result<()>;
    async fn list_tags(&self, scope: &ContextUri) -> Result<Vec<Tag>>;

    // === 读取 / 时间旅行 ===
    async fn log(&self, scope: &ContextUri, opts: &LogOpts) -> Result<Vec<Commit>>;
    async fn read_at(
        &self,
        uri: &ContextUri,
        ref_: VersionRef,
        level: ContentLevel,
    ) -> Result<ContentPayload>;
    async fn asof_read(
        &self,
        uri: &ContextUri,
        when: AsOfTime,
        level: ContentLevel,
    ) -> Result<ContentPayload>;

    // === 合并 / Diff ===
    async fn merge(
        &self,
        scope: &ContextUri,
        from: &BranchName,
        into: &BranchName,
        strategy: MergeStrategy,
    ) -> Result<MergeResult>;
    async fn diff_commits(
        &self,
        scope: &ContextUri,
        a: &CommitId,
        b: &CommitId,
    ) -> Result<TreeDiff>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_dag_parents() {
        let root = CommitId::new();
        let merge = Commit {
            id: CommitId::new(),
            parents: vec![root.clone(), CommitId::new()],
            tree_hash: ContentHash("abc".into()),
            author: Author {
                agent_id: Some("a1".into()),
                user_id: None,
                system: false,
            },
            message: "merge".into(),
            timestamp: chrono::Utc::now(),
            metadata: CommitMeta {
                trigger: CommitTrigger::Merge {
                    branches: vec![BranchName("main".into())],
                },
                changes: ChangeSet::default(),
                provenance: vec![],
            },
        };
        assert_eq!(merge.parents.len(), 2);
        assert_eq!(merge.parents[0], root);
    }
}
