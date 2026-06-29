//! # agent-metacognition
//!
//! 元认知 —— 三信号在线自校准 + TTS（Time To Stop）渐进式预算控制。
//!
//! ## 三信号融合
//!
//! ```text
//! meta_score = w1 × verifier + w2 × (1 - pred_error) + w3 × cost_remaining
//! w1=0.5, w2=0.3, w3=0.2（可配置）
//! ```
//!
//! | 信号 | 来源 | 成本 |
//! |---|---|---|
//! | verifier | 本地校准模型（Qwen2.5-0.5B 或 mock） | ~50ms |
//! | pred_error | State fork() 推演 | 零 LLM call |
//! | cost_remaining | 计数器 | 零 LLM call |
//!
//! ## 在线 vs Sidecar 分工
//!
//! | | Metacognition（在线） | Monitor（Sidecar） |
//! |---|---|---|
//! | 触发 | 每步决策后 | 异步、节流（60s+ 异常） |
//! | 延迟要求 | < 200ms | 无要求 |
//! | 职责 | 单步校准："这一步对吗？" | 全局检测："最近在退化吗？" |
//!
//! ## TTS 渐进式预算控制
//!
//! | 级别 | cost_remaining | 行为 |
//! |---|---|---|
//! | Normal | > 0.5 | 正常推理，ToT beam search 允许 |
//! | Degraded | 0.2-0.5 | 禁用 ToT，单步推理 |
//! | Urgent | 0.05-0.2 | 只走 Reaction + 直接回答，禁止新工具调用 |
//! | Abort | < 0.05 | 预算耗尽，终止 |

mod anomaly;
mod calibrate;
mod evaluate;
mod history;
mod tts;

pub use anomaly::AnomalyDetector;
pub use calibrate::{CalibrationModel, CalibrationResult};
pub use evaluate::{Metacognition, MetacognitiveAssessment, MetaScoreWeights};
pub use history::{CalibrationHistory, CalibrationRecord};
pub use tts::{TTSSignal, compute_cost_remaining_from_budget};


use serde::{Deserialize, Serialize};

/// 元认知动作 —— Metacognition 评估后对主循环的建议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetaAction {
    /// 正常放行
    Proceed,
    /// 回滚 State，重新推理
    RetryDecision,
    /// 暂停，向用户提问
    RequestClarification,
    /// 切换推理模式（含模式检测）
    SwitchStrategy,
    /// 升级给人类
    DelegateToHuman,
    /// 预算耗尽，终止
    AbortOnBudget,
}
