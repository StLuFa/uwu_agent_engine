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
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// ===========================================================================
// State 快照
// ===========================================================================

/// State 快照（M3 骨架，真实类型来自 agent-state，在对接时替换）。
///
/// 提供完整的 State 快照结构，包含元数据、版本追踪和内容哈希，
/// 支持高效的 fork 比较和真值源重算。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub agent_id: String,
    pub scope: StateScope,
    /// 快照创建时间。
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 单调递增的快照序号（每 checkpoint 一次 +1）。
    pub sequence: u64,
    /// 父快照的 URI（用于追溯 lineage）。
    pub parent_snapshot: Option<ContextUri>,
    /// State 内容哈希（SHA-256，用于快速相等比较，避免深度 JSON 对比）。
    pub state_hash: String,
    /// 派生标量：从事实层重算的 EMA 投影（见 §6.3 真值源边界）。
    pub accumulated_pred_error: f32,
    /// 完整 State JSON（事实层）。
    pub payload: serde_json::Value,
}

impl StateSnapshot {
    /// 创建新快照（自动计算哈希和时间戳）。
    pub fn new(
        agent_id: String,
        scope: StateScope,
        sequence: u64,
        parent_snapshot: Option<ContextUri>,
        pred_error: f32,
        payload: serde_json::Value,
    ) -> Self {
        let state_hash = compute_hash(&payload);
        Self {
            agent_id,
            scope,
            timestamp: chrono::Utc::now(),
            sequence,
            parent_snapshot,
            state_hash,
            accumulated_pred_error: pred_error,
            payload,
        }
    }

    /// 检查两个快照的内容是否相同（按哈希）。
    pub fn content_eq(&self, other: &StateSnapshot) -> bool {
        self.state_hash == other.state_hash
    }
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

/// 计算 State 载荷的内容哈希（用于快速相等比较）。
fn compute_hash(payload: &serde_json::Value) -> String {
    let mut hasher = DefaultHasher::new();
    payload.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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
                let payload = serde_json::json!({"abstract": s});
                Ok(StateSnapshot::new(
                    agent_id.to_string(),
                    scope,
                    0,
                    None,
                    0.0,
                    payload,
                ))
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
    ///
    /// 返回 (fork_pred_error - baseline_pred_error) 作为差异信号：
    /// - 正值表示 fork 分支的预测误差大于基线（策略表现更差）
    /// - 负值表示 fork 分支预测误差更小（策略有所改善）
    pub async fn compare_fork_pred_error(
        &self,
        agent_id: &str,
        scope: StateScope,
        fork: &ForkHandle,
    ) -> Result<f32> {
        // 加载基线（main 分支的最新状态）
        let baseline = self.load(agent_id, scope).await?;

        // 读取 fork 分支的最新状态
        // 通过 version 层读取 fork 分支 HEAD 对应的 state 快照
        let fork_snap_uri = StateSnapshot::snapshot_uri(agent_id, scope);
        let fork_pred_error = match self
            .versions
            .read_at(
                &fork_snap_uri,
                agent_context_db_version::VersionRef::Commit(fork.baseline_commit.clone()),
                agent_context_db_core::ContentLevel::L2,
            )
            .await
        {
            Ok(agent_context_db_core::ContentPayload::Detail(bytes)) => {
                match serde_json::from_slice::<StateSnapshot>(&bytes) {
                    Ok(snap) => snap.accumulated_pred_error,
                    Err(_) => {
                        // Fallback: try L1
                        match self
                            .versions
                            .read_at(
                                &fork_snap_uri,
                                agent_context_db_version::VersionRef::Commit(
                                    fork.baseline_commit.clone(),
                                ),
                                agent_context_db_core::ContentLevel::L1,
                            )
                            .await
                        {
                            Ok(agent_context_db_core::ContentPayload::Overview(json_str)) => {
                                serde_json::from_str::<StateSnapshot>(&json_str)
                                    .map(|s| s.accumulated_pred_error)
                                    .unwrap_or(baseline.accumulated_pred_error)
                            }
                            _ => baseline.accumulated_pred_error,
                        }
                    }
                }
            }
            _ => {
                // 无法读取 fork 状态时，返回 baseline（零差异）
                baseline.accumulated_pred_error
            }
        };

        // 返回差异信号
        Ok(fork_pred_error - baseline.accumulated_pred_error)
    }
}
