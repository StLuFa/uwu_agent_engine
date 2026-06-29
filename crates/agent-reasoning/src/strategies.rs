//! ReasoningStrategy —— 根据 TTSSignal 切换推理策略

use serde::{Deserialize, Serialize};

/// 推理策略 —— 对应 TTS 信号的不同推理深度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningStrategy {
    /// 正常推理：ToT beam search 多候选推演
    Normal,
    /// 降级推理：单步推理，禁用 ToT
    Degraded,
    /// 紧急推理：直接回答，禁止新工具调用
    Urgent,
    /// 终止：预算耗尽
    Abort,
}

impl ReasoningStrategy {
    /// 根据 TTS 信号的 cost_remaining 选择策略
    ///
    /// - cost > 0.5 → Normal
    /// - 0.2 < cost ≤ 0.5 → Degraded
    /// - 0.05 < cost ≤ 0.2 → Urgent
    /// - cost ≤ 0.05 → Abort
    pub fn from_cost_remaining(cost: f32) -> Self {
        if cost <= 0.05 {
            Self::Abort
        } else if cost <= 0.2 {
            Self::Urgent
        } else if cost <= 0.5 {
            Self::Degraded
        } else {
            Self::Normal
        }
    }

    /// 是否允许 ToT
    pub fn allows_tot(&self) -> bool {
        matches!(self, Self::Normal)
    }

    /// 是否允许新工具调用
    pub fn allows_new_tools(&self) -> bool {
        matches!(self, Self::Normal | Self::Degraded)
    }

    /// 是否应终止
    pub fn should_abort(&self) -> bool {
        matches!(self, Self::Abort)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_at_high_cost() {
        assert_eq!(
            ReasoningStrategy::from_cost_remaining(0.8),
            ReasoningStrategy::Normal
        );
    }

    #[test]
    fn degraded_at_mid_cost() {
        assert_eq!(
            ReasoningStrategy::from_cost_remaining(0.3),
            ReasoningStrategy::Degraded
        );
    }

    #[test]
    fn urgent_at_low_cost() {
        assert_eq!(
            ReasoningStrategy::from_cost_remaining(0.1),
            ReasoningStrategy::Urgent
        );
    }

    #[test]
    fn abort_at_zero() {
        assert_eq!(
            ReasoningStrategy::from_cost_remaining(0.0),
            ReasoningStrategy::Abort
        );
    }
}
