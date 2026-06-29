//! CalibrationModel + CalibrationResult

use agent_state::AgentState;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 校准结果 —— 独立校准模型对当前决策的评估
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationResult {
    /// 模型原始置信度 [0.0, 1.0]
    pub raw_confidence: f32,
    /// 校准后的置信度 [0.0, 1.0]
    pub calibrated_confidence: f32,
    /// 是否应重试决策
    pub should_retry: bool,
    /// 校准模型的推理说明
    pub reasoning: String,
}

/// 校准模型 trait —— 评估决策质量
///
/// 独立于主 LLM，用本地小模型（如 Qwen2.5-0.5B）或规则校验，
/// 成本 ≈ LLM call 的 1/100。
#[async_trait]
pub trait CalibrationModel: Send + Sync {
    /// 校准当前决策
    ///
    /// `decision_text` 是决策的文本表示（因 agent-reasoning::Decision 尚未实现，解耦）。
    async fn calibrate(
        &self,
        state: &AgentState,
        decision_text: &str,
    ) -> CalibrationResult;
}
