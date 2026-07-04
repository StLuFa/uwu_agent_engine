//! `SessionCompressorImpl`：两阶段 commit 会话压缩器实现。
//!
//! - Phase1（同步）：归档消息 → 写入 FS → 返回 task_id
//! - Phase2（异步骨架）：标记完成 → 实际语义处理由上层编排注入

use agent_context_db_core::{
    ContentRepo, ContentType, ContextEntry, ContextMeta, ContextUri, MvccVersion, Result, TenantId,
};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    CommitTaskId, DoneMarker, SessionCompressor,
    SessionHandle, TaskStatus,
};

/// 会话压缩器实现。
///
/// Phase1 将消息写入 FS 归档目录；Phase2 是占位（标记 done），
/// 实际语义处理（L0/L1 生成、记忆提取）由上层通过 `MemoryExtractor` + `SemanticProcessor` 编排。
pub struct SessionCompressorImpl {
    store: Arc<dyn ContentRepo>,
    /// Phase1 → Phase2 间暂存的会话句柄。
    pending: Mutex<HashMap<CommitTaskId, PendingSession>>,
}

pub struct PendingSession {
    pub handle: SessionHandle,
    pub archive_uri: ContextUri,
}

impl SessionCompressorImpl {
    pub fn new(store: Arc<dyn ContentRepo>) -> Self {
        Self {
            store,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// 获取暂存的会话句柄（供上层编排 Phase2 语义处理）。
    pub fn take_pending(&self, task_id: &CommitTaskId) -> Option<PendingSession> {
        self.pending.lock().remove(task_id)
    }

    /// 归档 URI：`{archive_dir}/{compression_index}/messages.jsonl`
    pub fn archive_file_uri(session: &SessionHandle) -> ContextUri {
        session
            .archive_dir
            .join(&session.compression_index.to_string())
            .join("messages.jsonl")
    }
}

#[async_trait]
impl SessionCompressor for SessionCompressorImpl {
    async fn commit_phase1(&self, session: &SessionHandle) -> Result<CommitTaskId> {
        let task_id = CommitTaskId::new();
        let archive_uri = Self::archive_file_uri(session);

        // 将消息序列化为 JSONL
        let mut jsonl = String::new();
        for msg in &session.messages {
            let line = serde_json::to_string(msg).unwrap_or_default();
            jsonl.push_str(&line);
            jsonl.push('\n');
        }

        // 写入归档条目
        let entry = ContextEntry {
            uri: archive_uri.clone(),
            tenant: TenantId(uuid::Uuid::nil()),
            l0_abstract: format!(
                "session {} compression #{} with {} messages",
                session.session_id,
                session.compression_index,
                session.messages.len()
            ),
            l1_overview: Some(jsonl.clone()),
            l2_detail_uri: None,
            content_type: ContentType::Text,
            metadata: ContextMeta::default(),
            mvcc_version: MvccVersion(0),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.store.write(entry).await?;

        // 暂存会话句柄供 Phase2 使用
        self.pending.lock().insert(
            task_id,
            PendingSession {
                handle: session.clone(),
                archive_uri,
            },
        );

        Ok(task_id)
    }

    async fn commit_phase2(&self, task_id: CommitTaskId) -> Result<DoneMarker> {
        // Phase2 骨架：上层负责在 Phase1 和 Phase2 之间调用语义管线
        // (MemoryExtractor + SemanticProcessor)，完成后再调用此方法标记完成。
        let pending = self
            .take_pending(&task_id)
            .ok_or_else(|| {
                agent_context_db_core::ContextError::NotFound(format!(
                    "no pending session for task {task_id:?}"
                ))
            })?;

        // 写 .done 标记
        let done_uri = pending
            .handle
            .archive_dir
            .join(&pending.handle.compression_index.to_string())
            .join(".done");

        let done_marker = DoneMarker {
            task_id,
            finished_at: chrono::Utc::now(),
            abstract_uri: pending.archive_uri.clone(),
            overview_uri: pending.archive_uri.clone(), // 简化：同一文件包含 L0+L1
            memory_diff_uri: Some(
                pending
                    .handle
                    .archive_dir
                    .join(&pending.handle.compression_index.to_string())
                    .join("memory_diff.json"),
            ),
        };

        let done_entry = ContextEntry {
            uri: done_uri,
            tenant: TenantId(uuid::Uuid::nil()),
            l0_abstract: format!("done: {done_marker:?}"),
            l1_overview: None,
            l2_detail_uri: None,
            content_type: ContentType::Text,
            metadata: ContextMeta::default(),
            mvcc_version: MvccVersion(0),
            created_at: done_marker.finished_at,
            updated_at: done_marker.finished_at,
        };

        self.store.write(done_entry).await?;

        Ok(done_marker)
    }

    async fn poll_task(&self, task_id: CommitTaskId) -> Result<TaskStatus> {
        match self.pending.lock().get(&task_id) {
            Some(_) => Ok(TaskStatus::Processing),
            None => Ok(TaskStatus::Failed("task not found or already completed".into())),
        }
    }
}

// ===========================================================================
// 高层编排函数（组合 SessionCompressor + 语义管线）
// ===========================================================================

/// 完整的两阶段 commit 编排：Phase1 归档 → Phase2 语义处理。
///
/// `extractor` 和 `semantic` 由上层注入（MemoryContextStore / MockLlmClient）。
pub async fn run_full_compression(
    compressor: &SessionCompressorImpl,
    extractor: &dyn crate::MemoryExtractorShim,
    session: &SessionHandle,
) -> Result<DoneMarker> {
    // Phase1
    let task_id = compressor.commit_phase1(session).await?;

    // Phase2 语义处理（由上层编排）
    let pending = compressor
        .take_pending(&task_id)
        .ok_or_else(|| {
            agent_context_db_core::ContextError::NotFound("task disappeared".into())
        })?;

    // 提取记忆
    let _candidates = extractor.extract(&pending.archive_uri).await?;
    // 去重
    // let decisions = extractor.deduplicate(candidates).await?;
    // 生成 L0/L1
    // semantic.generate_abstract(...).await?;

    // 标记完成
    let done = compressor.commit_phase2(task_id).await?;
    Ok(done)
}

/// Phase2 语义处理的 trait shim，避免 session crate 直接依赖 parse crate。
#[async_trait]
pub trait MemoryExtractorShim: Send + Sync {
    async fn extract(&self, archive: &ContextUri) -> Result<Vec<String>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role, SessionMessage};

    fn make_session() -> SessionHandle {
        SessionHandle {
            session_id: uuid::Uuid::new_v4(),
            user_id: "u1".into(),
            agent_id: "a1".into(),
            messages: vec![
                SessionMessage {
                    role: Role::User,
                    content: "hello".into(),
                    timestamp: chrono::Utc::now(),
                    metadata: serde_json::Value::Null,
                },
                SessionMessage {
                    role: Role::Assistant,
                    content: "hi there".into(),
                    timestamp: chrono::Utc::now(),
                    metadata: serde_json::Value::Null,
                },
            ],
            compression_index: 0,
            archive_dir: ContextUri::parse("uwu://t1/sessions/s1/archive").unwrap(),
        }
    }

    #[test]
    fn archive_uri_contains_index_and_filename() {
        let s = make_session();
        let uri = SessionCompressorImpl::archive_file_uri(&s);
        assert!(uri.to_string().contains("messages.jsonl"));
        assert!(uri.to_string().contains("/0/"));
    }
}
