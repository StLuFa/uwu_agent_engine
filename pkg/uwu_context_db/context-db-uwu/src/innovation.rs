//! 专有扩展创新功能（F18 跨 Agent 联邦 + F29 多模态对齐）。

use agent_context_db_core::{ContextUri, LlmClient, LlmOpts};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// F18 跨 Agent 联邦
// ═══════════════════════════════════════════════════════════════════════════

/// 联邦共享的上下文条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedEntry {
    /// 来源 Agent
    pub source_agent: String,
    /// 原始 URI
    pub uri: ContextUri,
    /// L0 摘要
    pub abstract_: String,
    /// 共享策略
    pub sharing_policy: SharingPolicy,
    /// 时间戳
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SharingPolicy {
    Public,
    TrustedPeers { allowed_agents: Vec<String> },
    Anonymous,
    Private,
}

/// 联邦上下文视图 —— 一个 Agent 可见的跨 Agent 上下文。
#[derive(Debug, Clone, Default)]
pub struct FederatedView {
    pub entries: Vec<FederatedEntry>,
}

/// 联邦协议 —— 跨 Agent 上下文交换。
///
/// 支持 push（广播自己的上下文）和 pull（查询其他 Agent 的公开上下文）。
pub struct FederationProtocol {
    /// 本地 Agent ID
    local_agent: String,
    /// 已知的对等 Agent
    peers: parking_lot::Mutex<Vec<String>>,
    /// 联邦条目缓存
    cache: parking_lot::Mutex<Vec<FederatedEntry>>,
}

impl FederationProtocol {
    pub fn new(local_agent: impl Into<String>) -> Self {
        Self {
            local_agent: local_agent.into(),
            peers: parking_lot::Mutex::new(Vec::new()),
            cache: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// 注册一个对等 Agent。
    pub fn register_peer(&self, agent_id: impl Into<String>) {
        self.peers.lock().push(agent_id.into());
    }

    /// 推送一个上下文条目到联邦。
    pub fn push(&self, entry: FederatedEntry) {
        self.cache.lock().push(entry);
    }

    /// 从联邦拉取与查询相关的公开条目。
    pub fn pull(&self, query: &str) -> Vec<FederatedEntry> {
        self.cache
            .lock()
            .iter()
            .filter(|e| {
                matches!(e.sharing_policy, SharingPolicy::Public | SharingPolicy::Anonymous)
                    && e.abstract_.to_lowercase().contains(&query.to_lowercase())
            })
            .cloned()
            .collect()
    }

    /// 获取联邦状态摘要。
    pub fn status(&self) -> FederationStatus {
        let cache = self.cache.lock();
        FederationStatus {
            peer_count: self.peers.lock().len(),
            shared_entries: cache.len(),
            public_entries: cache.iter().filter(|e| matches!(e.sharing_policy, SharingPolicy::Public)).count(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FederationStatus {
    pub peer_count: usize,
    pub shared_entries: usize,
    pub public_entries: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// F29 多模态对齐
// ═══════════════════════════════════════════════════════════════════════════

/// 模态类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
}

/// 多模态对齐结果。
#[derive(Debug, Clone)]
pub struct AlignmentResult {
    /// 源模态
    pub source_modality: Modality,
    /// 目标模态
    pub target_modality: Modality,
    /// 对齐文本描述
    pub description: String,
    /// 对齐质量 0-1
    pub quality: f32,
}

/// 多模态对齐器 —— 在文本描述和其他模态之间建立映射。
///
/// 当前实现：文本 ↔ 文本（全模态对齐的基础层）。
/// 完整多模态需要外部 VLM/ASR 模型。
pub struct MultimodalAligner {
    llm: Arc<dyn LlmClient>,
}

impl MultimodalAligner {
    pub fn new(llm: Arc<dyn LlmClient>) -> Self {
        Self { llm }
    }

    /// 生成跨模态对齐描述。
    ///
    /// 例如：输入"红色的苹果" → 生成图像描述 prompt。
    pub async fn align_text_to_visual(
        &self,
        text: &str,
    ) -> std::result::Result<String, agent_context_db_core::LlmError> {
        let prompt = format!(
            r#"Convert this text description into a detailed visual description suitable for image generation:

Text: "{text}"

Describe: colors, shapes, spatial relationships, lighting, perspective, style.
Respond with ONLY the visual description."#
        );
        self.llm.complete(&prompt, &LlmOpts::default()).await
    }

    /// 将视觉描述转换回结构化文本。
    pub async fn align_visual_to_text(
        &self,
        visual_description: &str,
    ) -> std::result::Result<String, agent_context_db_core::LlmError> {
        let prompt = format!(
            r#"Extract structured information from this visual description:

Visual: "{visual_description}"

Return: what objects are present, their properties, actions, and relationships.
Respond with ONLY the structured summary."#
        );
        self.llm.complete(&prompt, &LlmOpts::default()).await
    }

    /// 判断两个模态的内容是否指向同一事物（跨模态去重）。
    pub async fn check_cross_modal_equivalence(
        &self,
        text_desc: &str,
        other_text_desc: &str,
    ) -> std::result::Result<f32, agent_context_db_core::LlmError> {
        let prompt = format!(
            r#"Are these two descriptions referring to the same thing?

Description A: "{text_desc}"
Description B: "{other_text_desc}"

Return a JSON with "same": true/false and "confidence": 0.0-1.0."#
        );
        let response = self.llm.complete(&prompt, &LlmOpts::default()).await?;

        #[derive(serde::Deserialize)]
        struct EqResult { same: bool, confidence: f32 }

        Ok(serde_json::from_str::<EqResult>(&response)
            .map(|r| if r.same { r.confidence } else { 1.0 - r.confidence })
            .unwrap_or(0.5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn federation_push_and_pull() {
        let fed = FederationProtocol::new("agent_a");
        fed.register_peer("agent_b");

        fed.push(FederatedEntry {
            source_agent: "agent_a".into(),
            uri: ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap(),
            abstract_: "solved memory leak in websocket handler".into(),
            sharing_policy: SharingPolicy::Public,
            timestamp: 1_700_000_000,
        });

        let results = fed.pull("memory leak");
        assert_eq!(results.len(), 1);

        let status = fed.status();
        assert_eq!(status.peer_count, 1);
        assert_eq!(status.public_entries, 1);
    }

    #[test]
    fn federation_private_entries_not_pulled() {
        let fed = FederationProtocol::new("agent_a");
        fed.push(FederatedEntry {
            source_agent: "agent_a".into(),
            uri: ContextUri::parse("uwu://t/agent/a/memories/preferences/p1").unwrap(),
            abstract_: "prefers secret configs".into(),
            sharing_policy: SharingPolicy::Private,
            timestamp: 1_700_000_000,
        });

        assert!(fed.pull("secret").is_empty());
    }

    #[test]
    fn federation_trusted_peers() {
        let fed = FederationProtocol::new("agent_a");
        let entry = FederatedEntry {
            source_agent: "agent_a".into(),
            uri: ContextUri::parse("uwu://t/x").unwrap(),
            abstract_: "trusted data".into(),
            sharing_policy: SharingPolicy::TrustedPeers { allowed_agents: vec!["agent_b".into()] },
            timestamp: 1_700_000_000,
        };
        fed.push(entry);

        // TrustedPeers 不在 Public/Anonymous → pull 不应返回
        assert!(fed.pull("trusted").is_empty());
    }
}
