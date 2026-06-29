//! MidTermWS + InteractionPattern —— 每 N 步更新的中程工作状态

use agent_types_core::{Action, ActionStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 动作记录 —— 记录已执行的动作及其状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub action: Action,
    pub status: ActionStatus,
    pub timestamp: DateTime<Utc>,
    pub outcome: Option<String>,
}

impl ActionRecord {
    pub fn new(action: Action, status: ActionStatus) -> Self {
        Self {
            action,
            status,
            timestamp: Utc::now(),
            outcome: None,
        }
    }

    pub fn with_outcome(mut self, outcome: impl Into<String>) -> Self {
        self.outcome = Some(outcome.into());
        self
    }
}

/// Agent 世界模型中的已知事实
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub key: String,
    pub value: String,
    /// 置信度 [0.0, 1.0]
    pub confidence: f32,
    pub established_at: DateTime<Utc>,
}

impl Fact {
    pub fn new(key: impl Into<String>, value: impl Into<String>, confidence: f32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            key: key.into(),
            value: value.into(),
            confidence: confidence.clamp(0.0, 1.0),
            established_at: Utc::now(),
        }
    }
}

/// 约束类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstraintType {
    /// 禁止执行特定命令
    ProhibitedAction { command: String },
    /// 值范围约束
    ValueBound {
        key: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// 必须包含某个事实
    MustInclude { fact_key: String },
}

/// Agent 动作空间上的约束
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub id: String,
    pub description: String,
    pub constraint_type: ConstraintType,
}

impl Constraint {
    pub fn new(description: impl Into<String>, constraint_type: ConstraintType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            constraint_type,
        }
    }
}

/// 检测到的交互模式 —— 被 Metacognition 消费
///
/// Metacognition::evaluate() 读取此结构以判断是否应切换策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionPattern {
    /// 最近 N 步的成功率
    pub recent_success_rate: f32,
    /// 检测到的模式名称（如 "loop_detected"）
    pub detected_pattern: Option<String>,
    /// 当前模式已持续的步数
    pub pattern_since_step: u32,
}

impl Default for InteractionPattern {
    fn default() -> Self {
        Self {
            recent_success_rate: 1.0,
            detected_pattern: None,
            pattern_since_step: 0,
        }
    }
}

impl InteractionPattern {
    /// 检测是否陷入失败循环
    /// - 成功率低于阈值，且
    /// - 持续步数超过阈值
    pub fn is_failure_loop(&self, success_threshold: f32, consecutive_steps: u32) -> bool {
        self.recent_success_rate < success_threshold
            && self.pattern_since_step >= consecutive_steps
    }

    /// 是否检测到循环模式
    pub fn is_loop_detected(&self) -> bool {
        self.detected_pattern.as_deref() == Some("loop_detected")
    }
}

/// 中程工作状态 —— 每 N 步更新
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidTermWS {
    /// MVCC 版本号，每 N 步 +1
    pub version: u64,
    /// 动作历史
    pub action_history: Vec<ActionRecord>,
    /// 已知事实
    pub known_facts: Vec<Fact>,
    /// 最近检测到的交互模式
    pub recent_pattern: Option<InteractionPattern>,
    /// 当前活跃的约束
    pub active_constraints: Vec<Constraint>,
}

impl Default for MidTermWS {
    fn default() -> Self {
        Self {
            version: 0,
            action_history: Vec::new(),
            known_facts: Vec::new(),
            recent_pattern: None,
            active_constraints: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_loop_detection() {
        let pattern = InteractionPattern {
            recent_success_rate: 0.2,
            detected_pattern: Some("loop_detected".into()),
            pattern_since_step: 5,
        };

        assert!(pattern.is_failure_loop(0.3, 5));
        assert!(pattern.is_loop_detected());
    }

    #[test]
    fn no_false_positive_for_short_sequence() {
        let pattern = InteractionPattern {
            recent_success_rate: 0.2,
            detected_pattern: None,
            pattern_since_step: 3,
        };

        // Low success rate but not enough consecutive steps
        assert!(!pattern.is_failure_loop(0.3, 5));
    }

    #[test]
    fn high_success_rate_no_detection() {
        let pattern = InteractionPattern {
            recent_success_rate: 0.8,
            detected_pattern: None,
            pattern_since_step: 10,
        };

        assert!(!pattern.is_failure_loop(0.3, 5));
    }
}
