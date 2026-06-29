//! TTSSignal + compute_cost_remaining

use agent_state::long::BudgetConsumed;
use chrono::Duration;
use serde::{Deserialize, Serialize};

/// TTS（Time To Stop）信号 —— 渐进式预算压力注入决策
///
/// 不是只有耗尽才停，而是在预算消耗到阈值时主动调整推理策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TTSSignal {
    /// 正常推理，ToT beam search 允许
    Normal,
    /// 降级：禁用 ToT，切换为单步推理
    Degraded {
        disable_tot: bool,
    },
    /// 紧急：只走 Reaction 短路 + 直接回答，禁止新工具调用
    Urgent {
        allow_reaction: bool,
        allow_new_tool: bool,
    },
    /// 预算耗尽，终止
    Abort,
}

/// 便捷函数 —— 委托给 BudgetConsumed::cost_remaining_fraction()
///
/// 三路预算取最紧张维度。
pub fn compute_cost_remaining_from_budget(
    b: &BudgetConsumed,
    max_tokens: u64,
    max_time: Duration,
    max_retries: u32,
) -> f32 {
    b.cost_remaining_fraction(max_tokens, max_time, max_retries)
}

/// 根据 cost_remaining 映射到 TTS 级别
///
/// - cost > 0.5 → Normal
/// - 0.2 < cost ≤ 0.5 → Degraded
/// - 0.05 < cost ≤ 0.2 → Urgent
/// - cost ≤ 0.05 → Abort
pub fn classify_tts(cost_remaining: f32) -> TTSSignal {
    match cost_remaining {
        c if c <= 0.05 => TTSSignal::Abort,
        c if c <= 0.2 => TTSSignal::Urgent {
            allow_reaction: true,
            allow_new_tool: false,
        },
        c if c <= 0.5 => TTSSignal::Degraded { disable_tot: true },
        _ => TTSSignal::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_normal() {
        assert_eq!(classify_tts(0.8), TTSSignal::Normal);
        assert_eq!(classify_tts(0.51), TTSSignal::Normal);
    }

    #[test]
    fn tts_degraded_boundary() {
        assert_eq!(
            classify_tts(0.5),
            TTSSignal::Degraded { disable_tot: true }
        );
        assert_eq!(
            classify_tts(0.21),
            TTSSignal::Degraded { disable_tot: true }
        );
    }

    #[test]
    fn tts_urgent_boundary() {
        assert_eq!(
            classify_tts(0.2),
            TTSSignal::Urgent {
                allow_reaction: true,
                allow_new_tool: false
            }
        );
        assert_eq!(
            classify_tts(0.06),
            TTSSignal::Urgent {
                allow_reaction: true,
                allow_new_tool: false
            }
        );
    }

    #[test]
    fn tts_abort_boundary() {
        assert_eq!(classify_tts(0.05), TTSSignal::Abort);
        assert_eq!(classify_tts(0.0), TTSSignal::Abort);
    }
}
