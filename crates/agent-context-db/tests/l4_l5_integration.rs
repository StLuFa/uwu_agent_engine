//! L4+L5 集成测试：SessionCompressor + MemoryExtractor + SemanticProcessor 全链路。
//!
//! 用 MemoryContextStore + MockLlmClient 验证完整的会话压缩→记忆提取→去重→摘要生成→写入。

use agent_context_db_core::{
    ContentRepo, ContextUri, FsOps, TenantId,
};
use agent_context_db_parse::{MemoryExtractor, MemoryExtractorImpl};
use agent_context_db_session::{
    Role, SessionCompressor, SessionCompressorImpl,
    SessionHandle, SessionMessage, TaskStatus,
};
use agent_context_db_testkit::MemoryContextStore;
use agent_context_db_uwu::MockLlmClient;
use std::sync::Arc;

fn make_session(msg_count: usize) -> SessionHandle {
    let mut messages = Vec::new();
    for i in 0..msg_count {
        messages.push(SessionMessage {
            role: if i % 2 == 0 { Role::User } else { Role::Assistant },
            content: format!("message {}: some content here for testing", i),
            timestamp: chrono::Utc::now(),
            metadata: serde_json::Value::Null,
        });
    }

    SessionHandle {
        session_id: uuid::Uuid::new_v4(),
        user_id: "u1".into(),
        agent_id: "a1".into(),
        messages,
        compression_index: 0,
        archive_dir: ContextUri::parse("uwu://t1/sessions/s1/archive").unwrap(),
    }
}

// ── Phase1 归档 ──────────────────────────────────────────────────────

#[tokio::test]
async fn phase1_archives_messages_to_fs() {
    let store = Arc::new(MemoryContextStore::new());
    let compressor = SessionCompressorImpl::new(store.clone());

    let session = make_session(3);
    let task_id = compressor.commit_phase1(&session).await.unwrap();

    // 验证：归档文件已写入
    let archive_uri = SessionCompressorImpl::archive_file_uri(&session);
    let content = store
        .read(&archive_uri, agent_context_db_core::ContentLevel::L1)
        .await
        .unwrap();

    match content {
        agent_context_db_core::ContentPayload::Overview(jsonl) => {
            assert!(jsonl.contains("message 0"));
            assert!(jsonl.contains("message 1"));
            assert!(jsonl.contains("message 2"));
        }
        other => panic!("expected Overview, got {:?}", other),
    }

    // pending 可用
    let pending = compressor.take_pending(&task_id);
    assert!(pending.is_some());
}

// ── Phase2 完成 ──────────────────────────────────────────────────────

#[tokio::test]
async fn phase2_writes_done_marker() {
    let store = Arc::new(MemoryContextStore::new());
    let compressor = SessionCompressorImpl::new(store.clone());

    let session = make_session(1);
    let task_id = compressor.commit_phase1(&session).await.unwrap();

    // Phase2
    let done = compressor.commit_phase2(task_id).await.unwrap();
    assert!(!done.abstract_uri.to_string().is_empty());

    // poll 应返回 Failed（task 已 take_pending 删除）
    let status = compressor.poll_task(task_id).await.unwrap();
    assert!(
        matches!(status, TaskStatus::Failed(_)),
        "task should be gone after phase2"
    );
}

// ── MemoryExtractor + MockLlmClient ───────────────────────────────────

#[tokio::test]
async fn memory_extractor_returns_candidates_from_mock_llm() {
    let llm = Arc::new(MockLlmClient);
    let extractor = MemoryExtractorImpl::new(llm);

    let archive = ContextUri::parse("uwu://t1/sessions/s1/archive/0/messages.jsonl").unwrap();
    let candidates = extractor.extract(&archive).await.unwrap();

    // Mock 应返回偏好+案例
    assert!(!candidates.is_empty(), "should return at least one candidate");
    assert!(
        candidates.iter().any(|c| c.content.contains("dark mode")),
        "should contain preference about dark mode: {:?}",
        candidates.iter().map(|c| &c.content).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn memory_extractor_deduplicate_with_mock_llm() {
    let llm = Arc::new(MockLlmClient);
    let extractor = MemoryExtractorImpl::new(llm);

    use agent_context_db_core::MemoryClass;
    let candidates = vec![
        agent_context_db_parse::MemoryCandidate {
            class: MemoryClass::Preferences,
            content: "likes dark mode".into(),
            source_uri: ContextUri::parse("uwu://t1/x").unwrap(),
            confidence: 0.9,
        },
        agent_context_db_parse::MemoryCandidate {
            class: MemoryClass::Cases,
            content: "fixed null pointer".into(),
            source_uri: ContextUri::parse("uwu://t1/y").unwrap(),
            confidence: 0.85,
        },
    ];

    let decisions = extractor.deduplicate(candidates).await.unwrap();
    assert_eq!(decisions.len(), 2, "should decide on both candidates");
}

// ── 完整两阶段管线（Phase1 + Mock 语义处理 + Phase2）───────────────

#[tokio::test]
async fn full_two_phase_compression_pipeline() {
    let store = Arc::new(MemoryContextStore::new());
    let compressor = SessionCompressorImpl::new(store.clone());
    let llm = Arc::new(MockLlmClient);
    let extractor = MemoryExtractorImpl::new(llm);

    let session = make_session(4);

    // Phase1: 归档
    let task_id = compressor.commit_phase1(&session).await.unwrap();

    // 计算归档 URI（不消耗 pending，留给 commit_phase2 使用）
    let archive_uri = SessionCompressorImpl::archive_file_uri(&session);

    // Phase2: 语义处理
    let candidates = extractor.extract(&archive_uri).await.unwrap();
    assert!(!candidates.is_empty(), "extraction should produce candidates");

    // 去重
    let decisions = extractor.deduplicate(candidates).await.unwrap();
    assert!(!decisions.is_empty(), "dedup should produce decisions");

    // 将去重结果写入 FS（模拟最终持久化）
    for (i, decision) in decisions.iter().enumerate() {
        let mem_uri = session
            .archive_dir
            .parent()
            .unwrap()
            .join(&format!("memory_{}", i));

        let entry = agent_context_db_core::ContextEntry::new_text(
            mem_uri,
            TenantId(uuid::Uuid::nil()),
            &decision.candidate.content,
        );
        store.write(entry).await.unwrap();
    }

    // Phase2: 标记完成
    let done = compressor.commit_phase2(task_id).await.unwrap();

    // 验证：done 标记已写入
    assert!(!done.abstract_uri.to_string().is_empty());
    assert!(done.memory_diff_uri.is_some());
}

// ── 归档 URI 构造 ────────────────────────────────────────────────────

#[test]
fn archive_uri_reflects_compression_index() {
    let session = make_session(1);
    let uri = SessionCompressorImpl::archive_file_uri(&session);
    assert!(uri.to_string().contains("/0/"));
    assert!(uri.to_string().contains("messages.jsonl"));
}
