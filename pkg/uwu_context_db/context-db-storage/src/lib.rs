//! # agent-context-db-storage (L7 存储层)
//!
//! 双层存储的**端口 + 装配根**：
//! - [`VectorIndex`]：Qdrant 索引层端口（URI+向量+元数据指针）。
//! - `ContextStore` 内容层端口来自 `core`（PG 真相源），此处不重复定义。
//! - [`ContextDbService`]：composition root —— 唯一同时持有内容层与索引层的地方。
//!
//! ## 解耦约束
//!
//! - 后端具体类型（PgPool / QdrantClient）**只在此层的适配器出现一次**，
//!   上层（retrieve/session/parse）只依赖 core 窄端口，永不感知后端。
//! - 本 crate 只给端口与装配骨架；真实 PG/Qdrant 适配器由宿主 crate 实现。

use agent_context_db_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ===========================================================================
// 索引层端口：向量检索（Qdrant 适配）
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexPoint {
    /// 指向内容层的 uwu:// URI 字符串。
    pub uri: String,
    pub vector: Vec<f32>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexHit {
    pub uri: String,
    pub score: f32,
    pub payload: serde_json::Value,
}

/// 索引层端口 —— 检索层通过它做向量召回，不感知 Qdrant。
#[async_trait]
pub trait VectorIndex: Send + Sync {
    async fn upsert(&self, collection: &str, point: IndexPoint) -> Result<()>;
    async fn search(
        &self,
        collection: &str,
        query: Vec<f32>,
        top_k: usize,
        filter: Option<serde_json::Value>,
    ) -> Result<Vec<IndexHit>>;
    async fn delete(&self, collection: &str, uri: &str) -> Result<()>;
}

// ===========================================================================
// 装配根：唯一持有内容层 + 索引层的地方
// ===========================================================================

/// composition root。内容层用 core 的 `ContextStore`（任意后端），
/// 索引层用本层 `VectorIndex`。上层拿到的是它暴露的窄端口。
pub struct ContextDbService<S> {
    content: Arc<S>,
    index: Arc<dyn VectorIndex>,
}

impl<S> ContextDbService<S>
where
    S: agent_context_db_core::ContextStore + 'static,
{
    pub fn new(content: Arc<S>, index: Arc<dyn VectorIndex>) -> Self {
        Self { content, index }
    }

    /// 交出内容层的只读寻址窄端口（供检索层使用）。
    pub fn fs_ops(&self) -> Arc<S> {
        self.content.clone()
    }

    /// 交出索引层端口。
    pub fn vector_index(&self) -> Arc<dyn VectorIndex> {
        self.index.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_context_db_core::{
        ContentLevel, ContentPayload, ContextEntry, ContextUri, DirEntry, FindPattern, GrepHit,
        MvccVersion, TenantId, TreeNode, VersionEntry, ContextDiff,
    };
    use agent_context_db_core::{ContentRepo, FsOps, TenantOps, VersionOps};

    #[derive(Default)]
    struct NoopStore;
    #[async_trait]
    impl FsOps for NoopStore {
        async fn ls(&self, _d: &ContextUri) -> Result<Vec<DirEntry>> { Ok(vec![]) }
        async fn find(&self, _p: &FindPattern) -> Result<Vec<ContextUri>> { Ok(vec![]) }
        async fn grep(&self, _r: &str, _s: &ContextUri) -> Result<Vec<GrepHit>> { Ok(vec![]) }
        async fn tree(&self, root: &ContextUri, _d: usize) -> Result<TreeNode> {
            Ok(TreeNode { uri: root.clone(), is_dir: true, children: vec![] })
        }
        async fn read(&self, _u: &ContextUri, _l: ContentLevel) -> Result<ContentPayload> {
            Ok(ContentPayload::Abstract(String::new()))
        }
    }
    #[async_trait]
    impl ContentRepo for NoopStore {
        async fn write(&self, _e: ContextEntry) -> Result<MvccVersion> { Ok(MvccVersion(1)) }
        async fn delete(&self, _u: &ContextUri) -> Result<()> { Ok(()) }
        async fn rename(&self, _f: &ContextUri, _t: &ContextUri) -> Result<()> { Ok(()) }
    }
    #[async_trait]
    impl VersionOps for NoopStore {
        async fn version_history(&self, _u: &ContextUri) -> Result<Vec<VersionEntry>> { Ok(vec![]) }
        async fn rollback(&self, _u: &ContextUri, _t: MvccVersion) -> Result<()> { Ok(()) }
        async fn diff(&self, _u: &ContextUri, _a: MvccVersion, _b: MvccVersion) -> Result<ContextDiff> {
            Ok(ContextDiff::default())
        }
    }
    #[async_trait]
    impl TenantOps for NoopStore {
        async fn list_tenants(&self) -> Result<Vec<TenantId>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct NoopIndex;
    #[async_trait]
    impl VectorIndex for NoopIndex {
        async fn upsert(&self, _c: &str, _p: IndexPoint) -> Result<()> { Ok(()) }
        async fn search(&self, _c: &str, _q: Vec<f32>, _k: usize, _f: Option<serde_json::Value>) -> Result<Vec<IndexHit>> { Ok(vec![]) }
        async fn delete(&self, _c: &str, _u: &str) -> Result<()> { Ok(()) }
    }

    #[tokio::test]
    async fn service_hands_out_ports() {
        let svc = ContextDbService::new(Arc::new(NoopStore), Arc::new(NoopIndex));
        let idx = svc.vector_index();
        idx.upsert("c", IndexPoint { uri: "uwu://t/x".into(), vector: vec![1.0], payload: serde_json::json!({}) }).await.unwrap();
        let fs = svc.fs_ops();
        assert!(fs.ls(&ContextUri::parse("uwu://t/agent/a").unwrap()).await.unwrap().is_empty());
    }
}
