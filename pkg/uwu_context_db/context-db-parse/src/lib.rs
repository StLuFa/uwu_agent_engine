//! # agent-context-db-parse (L5 解析层)
//!
//! 三个正交解析端口：
//! - [`SemanticProcessor`]：自底向上生成 L0/L1 + 多模态转文本
//! - [`MemoryExtractor`]：8 类记忆提取 + LLM 去重
//! - [`TrajectoryExtractor`]：会话→Trajectory，多轨迹→Experience
//!
//! ## 解耦约束
//!
//! - 仅依赖 core 类型与端口；不依赖 session/compressor（被它们调用，非反向）。
//! - 全部为端口（零实现），LLM 具体实现由 core 的 `LlmClient` 注入。

use agent_context_db_core::{ContextUri, MemoryClass, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ===========================================================================
// 语义处理器：自底向上生成 L0/L1
// ===========================================================================

#[async_trait]
pub trait SemanticProcessor: Send + Sync {
    async fn generate_abstract(&self, uri: &ContextUri) -> Result<String>;
    async fn generate_overview(&self, uri: &ContextUri) -> Result<String>;
    async fn aggregate_upward(&self, root: &ContextUri) -> Result<()>;
    /// 多模态 → (abstract, overview) 文本对。
    async fn multimodal_to_text(&self, uri: &ContextUri) -> Result<(String, String)>;
}

// ===========================================================================
// 记忆提取器：8 类分类 + LLM 去重
// ===========================================================================

#[async_trait]
pub trait MemoryExtractor: Send + Sync {
    async fn extract(&self, archive: &ContextUri) -> Result<Vec<MemoryCandidate>>;
    async fn deduplicate(&self, candidates: Vec<MemoryCandidate>) -> Result<Vec<DedupDecision>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub class: MemoryClass,
    pub content: String,
    pub source_uri: ContextUri,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupDecision {
    pub candidate: MemoryCandidate,
    pub action: CandidateAction,
    pub merge_target: Option<ContextUri>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateAction {
    Skip,
    Create,
    Merge,
    Delete,
    None,
}

// ===========================================================================
// 轨迹提取器：会话级 → Trajectory；多轨迹 → Experience
// ===========================================================================

#[async_trait]
pub trait TrajectoryExtractor: Send + Sync {
    async fn extract_trajectory(&self, archive: &ContextUri) -> Result<Trajectory>;
    async fn induce_experience(&self, trajectories: Vec<ContextUri>) -> Result<Experience>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    pub uri: ContextUri,
    pub session_id: Uuid,
    pub did_what: String,
    pub how: String,
    pub result: String,
    pub state_snapshot_uri: Option<ContextUri>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub uri: ContextUri,
    pub situation: String,
    pub approach: String,
    pub reflect: String,
    pub related_trajectories: Vec<ContextUri>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_decision_shapes() {
        let c = MemoryCandidate {
            class: MemoryClass::Preferences,
            content: "likes dark mode".into(),
            source_uri: ContextUri::parse("uwu://t/user/u/sessions/s1").unwrap(),
            confidence: 0.9,
        };
        let d = DedupDecision {
            candidate: c,
            action: CandidateAction::Merge,
            merge_target: Some(ContextUri::parse("uwu://t/user/u/memories/preferences/p1").unwrap()),
            reason: "same preference".into(),
        };
        assert_eq!(d.action, CandidateAction::Merge);
        assert!(d.merge_target.is_some());
    }
}
