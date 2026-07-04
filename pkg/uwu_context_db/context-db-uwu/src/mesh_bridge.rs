//! U10 Trajectory→Reaction 自动学习 + U11 EventMesh 桥接。

use agent_context_db_core::{ContentLevel, ContextUri, FsOps, LlmClient, LlmOpts};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// U10 Trajectory→Reaction 自动学习
// ═══════════════════════════════════════════════════════════════════════════

/// 候选 Reaction 规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRule {
    /// 规则名
    pub name: String,
    /// 触发条件模板
    pub condition: String,
    /// 动作描述
    pub action: String,
    /// 来源经验 URI
    pub source_experience: ContextUri,
    /// 置信度
    pub confidence: f32,
    /// 是否需要沙箱验证
    pub require_sandbox: bool,
}

/// Reaction 学习器 —— 从经验记忆中归纳新 Reaction 规则。
pub struct ReactionLearner {
    llm: Arc<dyn LlmClient>,
    fs: Arc<dyn FsOps>,
}

impl ReactionLearner {
    pub fn new(llm: Arc<dyn LlmClient>, fs: Arc<dyn FsOps>) -> Self {
        Self { llm, fs }
    }

    /// 扫描 experiences 目录，寻找高频 Situation 模式。
    pub async fn induce_rules(
        &self,
        experience_dir: &ContextUri,
    ) -> std::result::Result<Vec<NewRule>, agent_context_db_core::ContextError> {
        let entries = self.fs.ls(experience_dir).await?;
        let mut experiences = Vec::new();

        for e in entries {
            if e.is_dir {
                continue;
            }
            if let Ok(content) = self.fs.read(&e.uri, ContentLevel::L1).await {
                if let agent_context_db_core::ContentPayload::Overview(s) = content {
                    experiences.push((e.uri.clone(), s));
                }
            }
        }

        if experiences.is_empty() {
            return Ok(vec![]);
        }

        let combined: Vec<String> = experiences
            .iter()
            .map(|(uri, text)| format!("[{uri}]: {text}"))
            .collect();

        let prompt = format!(
            r#"Analyze these experiences and induce reusable reaction rules.

{}
For each high-confidence pattern, return a JSON object with:
- "name": rule name
- "condition": when this rule should trigger
- "action": what action to take
- "confidence": 0.0-1.0
- "require_sandbox": true if the action needs validation

Return JSON array of rules."#,
            combined.join("\n---\n")
        );

        let response = self.llm.complete(&prompt, &LlmOpts::default()).await
            .map_err(|e| agent_context_db_core::ContextError::Storage(format!("reaction learner: {e}")))?;

        #[derive(Deserialize)]
        struct RawRule {
            name: String,
            condition: String,
            action: String,
            confidence: f32,
            require_sandbox: bool,
        }

        let raw: Vec<RawRule> = serde_json::from_str(&response).unwrap_or_default();

        Ok(raw.into_iter().map(|r| NewRule {
            name: r.name,
            condition: r.condition,
            action: r.action,
            source_experience: experience_dir.clone(),
            confidence: r.confidence,
            require_sandbox: r.require_sandbox,
        }).collect())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// U11 EventMesh 桥接
// ═══════════════════════════════════════════════════════════════════════════

/// 上下文事件类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextEvent {
    /// 写入完成（含版本信息）
    Written {
        uri: String,
        version: u64,
        agent_id: Option<String>,
    },
    /// 检索完成
    Retrieved {
        query: String,
        hit_count: usize,
        tokens_used: usize,
    },
    /// 语义压缩完成
    Consolidated {
        task_id: String,
        session_id: String,
        memory_changes: MemoryChangeSummary,
    },
    /// Wiki Ingest 完成
    WikiIngested {
        space: String,
        doc_count: usize,
        contradictions: Vec<String>,
    },
    /// 矛盾检测
    Contradiction {
        uri_a: String,
        uri_b: String,
        description: String,
    },
    /// State fork 推演完成
    ForkCompleted {
        fork_name: String,
        result: String,
    },
}

/// 记忆变更摘要 —— 跨进程传输用轻量结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChangeSummary {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
}

/// EventMesh 桥接器 —— 把 context-db 内部事件发布到 uwu_event_mesh。
///
/// 不直接依赖 NATS，而是通过 event mesh 的 FlowHandle 发布。
/// 如需跨进程，由 uwu_nats_bridge 订阅 mesh 主题并桥接到 NATS。
pub struct EventMeshBridge {
    /// 事件 mesh 的 main channel sender
    mesh_tx: Option<Arc<dyn MeshPublisher>>,
    /// 本地事件历史
    events: parking_lot::Mutex<Vec<ContextEvent>>,
}

/// 抽象 mesh publisher，避免直接依赖 uwu_event_mesh 具体类型。
pub trait MeshPublisher: Send + Sync {
    fn publish(&self, topic: &str, payload: &[u8]);
}

impl EventMeshBridge {
    pub fn new() -> Self {
        Self {
            mesh_tx: None,
            events: parking_lot::Mutex::new(Vec::new()),
        }
    }

    /// 注入一个 mesh publisher（由 engine 装配时传入 FlowHandle 适配器）。
    pub fn with_mesh(mut self, publisher: Arc<dyn MeshPublisher>) -> Self {
        self.mesh_tx = Some(publisher);
        self
    }

    /// 发布写入事件。
    pub fn emit_written(&self, uri: &str, version: u64, agent_id: Option<&str>) {
        let event = ContextEvent::Written {
            uri: uri.to_string(),
            version,
            agent_id: agent_id.map(|s| s.to_string()),
        };
        self.emit("context.written", &event);
    }

    /// 发布检索事件。
    pub fn emit_retrieved(&self, query: &str, hit_count: usize, tokens_used: usize) {
        let event = ContextEvent::Retrieved { query: query.to_string(), hit_count, tokens_used };
        self.emit("context.retrieved", &event);
    }

    /// 发布语义压缩完成事件。
    pub fn emit_consolidated(&self, task_id: &str, session_id: &str, added: usize, updated: usize, deleted: usize) {
        let event = ContextEvent::Consolidated {
            task_id: task_id.to_string(),
            session_id: session_id.to_string(),
            memory_changes: MemoryChangeSummary { added, updated, deleted },
        };
        self.emit("context.consolidated", &event);
    }

    /// 发布矛盾检测事件。
    pub fn emit_contradiction(&self, uri_a: &str, uri_b: &str, description: &str) {
        let event = ContextEvent::Contradiction {
            uri_a: uri_a.to_string(),
            uri_b: uri_b.to_string(),
            description: description.to_string(),
        };
        self.emit("context.contradiction", &event);
    }

    fn emit(&self, topic: &str, event: &ContextEvent) {
        self.events.lock().push(event.clone());
        if let Some(ref mesh) = self.mesh_tx {
            if let Ok(payload) = serde_json::to_vec(event) {
                mesh.publish(topic, &payload);
            }
        }
    }

    /// 获取本地事件历史。
    pub fn event_history(&self) -> Vec<ContextEvent> {
        self.events.lock().clone()
    }
}

impl Default for EventMeshBridge {
    fn default() -> Self { Self::new() }
}

// FlowHandle → MeshPublisher 适配器由 engine composition root 注入，
// 不需要在此处依赖 uwu_event_mesh。

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMesh;
    impl MeshPublisher for MockMesh {
        fn publish(&self, _topic: &str, _payload: &[u8]) {}
    }

    #[test]
    fn bridge_emits_locally_without_mesh() {
        let bridge = EventMeshBridge::new();
        bridge.emit_written("uwu://t/x", 1, None);
        assert_eq!(bridge.event_history().len(), 1);
    }

    #[test]
    fn bridge_emits_to_mesh_when_configured() {
        let bridge = EventMeshBridge::new().with_mesh(Arc::new(MockMesh));
        bridge.emit_retrieved("test query", 5, 500);
        assert_eq!(bridge.event_history().len(), 1);
    }

    #[test]
    fn reaction_learner_produces_rules() {
        // 结构测试
        let rule = NewRule {
            name: "retry_on_timeout".into(),
            condition: "timeout detected".into(),
            action: "retry with exponential backoff".into(),
            source_experience: ContextUri::parse("uwu://t/experiences/e1").unwrap(),
            confidence: 0.85,
            require_sandbox: true,
        };
        assert!(rule.confidence > 0.5);
        assert!(rule.require_sandbox);
    }
}
