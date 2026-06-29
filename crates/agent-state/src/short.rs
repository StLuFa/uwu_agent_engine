//! ShortTermWS —— 每步更新的短程工作状态

use agent_types_core::Action;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 当前上下文的结构化描述
///
/// 定义在 agent-state 而非 agent-perception 以避免循环依赖。
/// agent-perception 的 PerceptionPipeline 产出此类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDescriptor {
    /// 上下文文本描述
    pub description: String,
    /// 从原始输入提取的结构化数据
    pub raw_data: serde_json::Value,
    /// 观测时间戳
    pub observed_at: DateTime<Utc>,
}

impl ContextDescriptor {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            raw_data: serde_json::Value::Null,
            observed_at: Utc::now(),
        }
    }

    /// 从原始输入创建（阶段 3 由 PerceptionPipeline 替代）
    pub fn from_raw(raw: &str) -> Self {
        Self::new(raw)
    }
}

impl Default for ContextDescriptor {
    fn default() -> Self {
        Self::new("")
    }
}

/// Agent 正在考虑的假设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub description: String,
    /// 置信度 [0.0, 1.0]
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
    /// 支持该假设的证据数量
    pub evidence_count: u32,
}

impl Hypothesis {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            confidence: 0.5,
            created_at: Utc::now(),
            evidence_count: 0,
        }
    }
}

/// 短程工作状态 —— 每步更新
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermWS {
    /// MVCC 版本号，每步 +1
    pub version: u64,
    /// 当前上下文描述
    pub current_context: ContextDescriptor,
    /// 上一步执行的动作
    pub last_action: Option<Action>,
    /// 上一步的观察结果
    pub last_observation: Option<String>,
    /// 暂存的假设列表
    pub pending_hypotheses: Vec<Hypothesis>,
}

impl Default for ShortTermWS {
    fn default() -> Self {
        Self {
            version: 0,
            current_context: ContextDescriptor::default(),
            last_action: None,
            last_observation: None,
            pending_hypotheses: Vec::new(),
        }
    }
}
