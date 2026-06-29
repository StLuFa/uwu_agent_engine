//! MemoryType / Memory / MemoryScore

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 记忆类型 —— 四种查询视图
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryType {
    /// 情景记忆 —— 具体经历/事件
    Episodic,
    /// 语义记忆 —— 事实/知识
    Semantic,
    /// 程序记忆 —— 技能/流程
    Procedural,
    /// 工作记忆 —— 当前上下文
    Working,
}

/// 记忆评分
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MemoryScore {
    /// 相似度 [0.0, 1.0]
    pub similarity: f32,
    /// 时效性权重 [0.0, 1.0]
    pub recency: f32,
    /// 访问频率权重 [0.0, 1.0]
    pub frequency: f32,
    /// 综合评分
    pub total: f32,
}

impl MemoryScore {
    pub fn new(similarity: f32, recency: f32, frequency: f32) -> Self {
        let total = (similarity + recency + frequency) / 3.0;
        Self {
            similarity: similarity.clamp(0.0, 1.0),
            recency: recency.clamp(0.0, 1.0),
            frequency: frequency.clamp(0.0, 1.0),
            total: total.clamp(0.0, 1.0),
        }
    }
}

/// 一条记忆记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// 记忆 ID
    pub id: String,
    /// 记忆类型
    pub memory_type: MemoryType,
    /// 文本内容
    pub content: String,
    /// 向量嵌入（简化：用 f32 数组表示）
    pub embedding: Vec<f32>,
    /// 综合评分
    pub score: MemoryScore,
    /// 关联的 State 快照 JSON（可选）
    pub state_snapshot_json: Option<String>,
    /// 关联的 Agent ID
    pub agent_id: Option<String>,
    /// 关联的 Task ID
    pub task_id: Option<String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 最后访问时间
    pub last_accessed: DateTime<Utc>,
    /// 访问次数
    pub access_count: u32,
}

impl Memory {
    pub fn new(
        memory_type: MemoryType,
        content: impl Into<String>,
        embedding: Vec<f32>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type,
            content: content.into(),
            embedding,
            score: MemoryScore::default(),
            state_snapshot_json: None,
            agent_id: None,
            task_id: None,
            created_at: now,
            last_accessed: now,
            access_count: 0,
        }
    }

    pub fn with_state(mut self, snapshot_json: impl Into<String>) -> Self {
        self.state_snapshot_json = Some(snapshot_json.into());
        self
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// 记录一次访问
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }
}
