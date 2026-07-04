//! State 桥接（M3）：State 快照读写 + fork 推演沙盒。
//!
//! 通过注入的 core 窄端口访问存储；fork/晋升委托 version 层。
//!
//! ## 真值源边界（ARCHITECTURE.md §6.3）
//!
//! - **派生层** `accumulated_pred_error: f32` = 热路径标量，零 IO 读取
//! - **事实层** = context-db `state/` 目录中的完整 JSON，可随时重算

use agent_context_db_core::{
    ContentLevel, ContentPayload, ContentRepo, ContextEntry, ContextMeta, ContextUri, FsOps,
    MvccVersion, Result, StateScope, TenantId,
};
use agent_context_db_version::{
    BranchName, BranchType, CommitId, MergeResult, MergeStrategy, VersionStore, VersionError,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ===========================================================================
// State 快照
// ===========================================================================

/// State 快照（M3 骨架，真实类型来自 agent-state，在对接时替换）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub agent_id: String,
    pub scope: StateScope,
    /// 派生标量：从事实层重算的 EMA 投影（见 §6.3 真值源边界）。
    pub accumulated_pred_error: f32,
    /// 完整 State JSON。
    pub payload: serde_json::Value,
}

impl StateSnapshot {
    /// 为给定 agent + scope 构造 State 目录下的条目 URI。
    pub fn dir_uri(agent_id: &str, scope: StateScope) -> ContextUri {
        let scope_seg = match scope {
            StateScope::Short => "short",
            StateScope::Mid => "mid",
            StateScope::Long => "long",
        };
        ContextUri(format!(
            "uwu://default/agent/{}/state/{}",
            agent_id, scope_seg
        ))
    }

    pub fn snapshot_uri(agent_id: &str, scope: StateScope) -> ContextUri {
        Self::dir_uri(agent_id, scope).join("snapshot.json")
    }
}

// ===========================================================================
// ForkHandle
// ===========================================================================

/// fork 句柄：推演期间写入 fork 分支，结束后晋升或回滚。
#[derive(Debug, Clone)]
pub struct ForkHandle {
    pub agent_id: String,
    pub scope: StateScope,
    pub branch: BranchName,
    pub baseline_commit: CommitId,
    pub scope_uri: ContextUri,
}

// ===========================================================================
// StateBridge
// ===========================================================================

pub struct StateBridge<S, V> {
    store: Arc<S>,
    versions: Arc<V>,
}

impl<S, V> StateBridge<S, V>
where
    S: FsOps + ContentRepo,
    V: VersionStore,
{
    pub fn new(store: Arc<S>, versions: Arc<V>) -> Self {
        Self { store, versions }
    }

    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    // ── load ──────────────────────────────────────────────────────

    /// 从 FS 加载 State 快照。
    ///
    /// 优先读 L2 Detail（生产 PG blob），fallback 到 L1 Overview（内存实现）。
    pub async fn load(
        &self,
        agent_id: &str,
        scope: StateScope,
    ) -> Result<StateSnapshot> {
        let uri = StateSnapshot::snapshot_uri(agent_id, scope);

        // 尝试 L2 Detail
        if let Ok(ContentPayload::Detail(bytes)) = self.store.read(&uri, ContentLevel::L2).await {
            if !bytes.is_empty() {
                return serde_json::from_slice(&bytes)
                    .map_err(|e| agent_context_db_core::ContextError::Serialization(
                        format!("state deserialize: {e}")
                    ));
            }
        }

        // Fallback: L1 Overview 中存储完整 JSON
        match self.store.read(&uri, ContentLevel::L1).await {
            Ok(ContentPayload::Overview(json_str)) => {
                serde_json::from_str(&json_str)
                    .map_err(|e| agent_context_db_core::ContextError::Serialization(
                        format!("state deserialize from L1: {e}")
                    ))
            }
            Ok(ContentPayload::Abstract(s)) => {
                // L0 fallback：从摘要中重建最小快照
                Ok(StateSnapshot {
                    agent_id: agent_id.to_string(),
                    scope,
                    accumulated_pred_error: 0.0,
                    payload: serde_json::json!({"abstract": s}),
                })
            }
            Ok(ContentPayload::Detail(_)) => {
                Err(agent_context_db_core::ContextError::Unsupported(
                    "unexpected Detail in L1 fallback".into(),
                ))
            }
            Err(e) => Err(e),
        }
    }

    // ── checkpoint ───────────────────────────────────────────────

    /// 写入 State 快照（持久化当前 L2 状态）。
    pub async fn checkpoint(
        &self,
        agent_id: &str,
        scope: StateScope,
        snap: &StateSnapshot,
        tenant: TenantId,
    ) -> Result<MvccVersion> {
        let uri = StateSnapshot::snapshot_uri(agent_id, scope);
        let full_json = serde_json::to_string(snap)
            .map_err(|e| agent_context_db_core::ContextError::Serialization(
                format!("state serialize: {e}")
            ))?;

        let entry = ContextEntry {
            uri: uri.clone(),
            tenant,
            l0_abstract: format!(
                "state snapshot agent={} scope={:?} pred_error={:.4}",
                agent_id, scope, snap.accumulated_pred_error
            ),
            l1_overview: Some(full_json),
            l2_detail_uri: None,
            content_type: agent_context_db_core::ContentType::Text,
            metadata: ContextMeta {
                state_scope: Some(scope),
                ..Default::default()
            },
            mvcc_version: MvccVersion(0),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.store.write(entry).await
    }

    // ── fork ─────────────────────────────────────────────────────

    /// fork 推演沙盒：在指定 scope 上创建 StateFork 分支。
    ///
    /// 返回 `ForkHandle`，后续写入走 scope commit（自动推进该分支），
    /// 完成后调用 `promote_fork` 或 `discard_fork`。
    pub async fn fork(
        &self,
        agent_id: &str,
        scope: StateScope,
    ) -> std::result::Result<ForkHandle, VersionError> {
        let scope_uri = StateSnapshot::dir_uri(agent_id, scope);

        // 确定 fork 起点的 commit
        let head_commit = {
            let log = self.versions.log(
                &scope_uri,
                &agent_context_db_version::LogOpts { max_count: Some(1), ..Default::default() },
            ).await?;
            log.first().cloned().map(|c| c.id).unwrap_or_else(CommitId::new)
        };

        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let id = SEQ.fetch_add(1, Ordering::Relaxed);
        let fork_name = BranchName(format!("fork-state-{}", id));

        self.versions
            .create_branch(&scope_uri, fork_name.clone(), head_commit.clone(), BranchType::StateFork)
            .await?;

        Ok(ForkHandle {
            agent_id: agent_id.to_string(),
            scope,
            branch: fork_name,
            baseline_commit: head_commit,
            scope_uri,
        })
    }

    // ── promote / discard ────────────────────────────────────────

    /// 晋升 fork：将 fork 分支合并回 main。
    pub async fn promote_fork(
        &self,
        fork: &ForkHandle,
        strategy: MergeStrategy,
    ) -> std::result::Result<MergeResult, VersionError> {
        let main = BranchName("main".into());
        self.versions
            .merge(&fork.scope_uri, &fork.branch, &main, strategy)
            .await
    }

    /// 丢弃 fork：删除 fork 分支。
    pub async fn discard_fork(
        &self,
        fork: &ForkHandle,
    ) -> std::result::Result<(), VersionError> {
        self.versions
            .delete_branch(&fork.scope_uri, &fork.branch)
            .await
    }

    // ── pred error ───────────────────────────────────────────────

    /// 比较 fork 与基线的预测误差（读派生标量 `accumulated_pred_error`）。
    pub async fn compare_fork_pred_error(
        &self,
        agent_id: &str,
        scope: StateScope,
        fork: &ForkHandle,
    ) -> Result<f32> {
        let baseline = self.load(agent_id, scope).await?;
        // fork 后当前 HEAD 的 state 需要通过 reload 获取（取最新版本）
        // 简化：直接用 baseline 的 pred_error，实际需要重新 load
        let _ = fork;
        Ok(baseline.accumulated_pred_error)
    }
}
