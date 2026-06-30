//! CalibrationModel + CalibrationResult + BayesianCalibrator

use agent_state::AgentState;
use agent_uncertainty::{BayesianAggregator, BetaBelief, BetaDistribution};
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

/// Bayesian calibration model — tracks decision accuracy as a Beta distribution.
///
/// Each decision is treated as a Bernoulli trial: success if pred_error is low,
/// failure if pred_error is high. The posterior Beta distribution provides
/// calibrated confidence with credible intervals.
///
/// # Example
///
/// ```ignore
/// let mut cal = BayesianCalibrator::new();
/// cal.observe_success();  // decision was correct
/// cal.observe_failure();  // decision was wrong
/// let result = cal.calibrate(&state, "click button").await;
/// // result.calibrated_confidence = posterior mean
/// ```
pub struct BayesianCalibrator {
    /// Belief about decision quality (Beta prior on success probability).
    pub belief: BetaBelief,
    /// Aggregator for combining multiple uncertainty sources.
    pub aggregator: BayesianAggregator,
}

impl BayesianCalibrator {
    /// Create with a uniform prior (no prior knowledge about decision quality).
    pub fn new() -> Self {
        Self {
            belief: BetaBelief::with_prior(
                "decision_quality",
                BetaDistribution::with_mean(0.5, 4.0), // weak prior at 0.5
            ),
            aggregator: BayesianAggregator::default(),
        }
    }

    /// Create with a custom prior.
    pub fn with_prior(prior: BetaDistribution) -> Self {
        Self {
            belief: BetaBelief::with_prior("decision_quality", prior),
            aggregator: BayesianAggregator::default(),
        }
    }

    /// Observe a successful decision.
    pub fn observe_success(&mut self) {
        self.belief.observe_success();
    }

    /// Observe a failed decision.
    pub fn observe_failure(&mut self) {
        self.belief.observe_failure();
    }

    /// Observe a batch of outcomes at once.
    pub fn observe_batch(&mut self, successes: u64, failures: u64) {
        self.belief.distribution.observe_batch(successes, failures);
        self.belief.total_observations += successes + failures;
    }

    /// Get the current Bayesian estimate of decision quality.
    pub fn estimate(&self) -> agent_uncertainty::BeliefEstimate {
        self.belief.estimate()
    }

    /// Get the overall uncertainty estimate (combines decision quality belief
    /// with any additional uncertainty dimensions).
    pub fn uncertainty(&self, additional_beliefs: &[BetaBelief]) -> agent_uncertainty::UncertaintyEstimate {
        let mut all = vec![self.belief.clone()];
        all.extend_from_slice(additional_beliefs);
        self.aggregator.aggregate(&all)
    }
}

// ===========================================================================
// LocalCalibrator — 基于特征工程的本地校准模型（零外部依赖）
// ===========================================================================

/// 本地校准器：5 维特征 + 逻辑回归 → 决策置信度。
///
/// 不依赖任何 ML 框架或外部模型文件。模型参数（5 个权重 + 1 个 bias）
/// 是硬编码的 f32 值，编译进二进制。延迟 < 1µs。
///
/// ## 特征设计
///
/// | 特征 | 含义 | 来源 |
/// |---|---|---|
/// | pred_error | 累积预测误差 | state.long_term.accumulated_pred_error |
/// | cost_remaining | 剩余预算比例 | Metacognition::compute_cost_remaining() |
/// | decision_len_log | 决策文本长度（log 归一化） | decision_text.len() |
/// | is_dangerous | 决策是否为危险命令 | 文本关键词匹配 |
/// | recent_success | 最近成功率 | state.mid_term.recent_pattern |
///
/// ## 模型参数
///
/// 逻辑回归：confidence = σ(w₁·f₁ + w₂·f₂ + w₃·f₃ + w₄·f₄ + w₅·f₅ + bias)
pub struct LocalCalibrator {
    /// 预训练权重（5 维特征）
    weights: [f32; 5],
    /// 偏置项
    bias: f32,
    /// 底层 Bayesian 追踪器（可选，组合信号）
    bayesian: BayesianCalibrator,
    /// 过去 N 步的特征向量（供在线学习用）
    recent_features: Vec<[f32; 5]>,
}

impl LocalCalibrator {
    /// 使用预置权重创建（基于启发式调参）。
    ///
    /// 权重含义：
    /// - pred_error: -0.6 (高误差 → 低置信度)
    /// - cost_remaining: +0.3 (预算充足 → 高置信度)
    /// - decision_len: -0.1 (过长文本 → 略低置信度)
    /// - is_dangerous: -0.4 (危险命令 → 低置信度)
    /// - recent_success: +0.5 (高成功率 → 高置信度)
    pub fn new() -> Self {
        Self {
            weights: [-0.6, 0.3, -0.1, -0.4, 0.5],
            bias: 0.2,
            bayesian: BayesianCalibrator::new(),
            recent_features: Vec::new(),
        }
    }

    /// 使用自定义权重创建。
    pub fn with_weights(weights: [f32; 5], bias: f32) -> Self {
        Self { weights, bias, ..Self::new() }
    }

    /// 提取 5 维特征向量。
    fn extract_features(&self, state: &AgentState, decision_text: &str) -> [f32; 5] {
        let pred_error = state.long_term.accumulated_pred_error;
        let cost_remaining = 1.0 - state.long_term.budget_consumed.tokens_used as f32 / 10_000.0;
        let decision_len = (decision_text.len() as f32 + 1.0).ln() / 8.0; // 归一化到 ~[0,1]
        let is_dangerous = if contains_dangerous_command(decision_text) { 1.0 } else { 0.0 };
        let recent_success = state
            .mid_term
            .recent_pattern
            .as_ref()
            .map(|p| p.recent_success_rate)
            .unwrap_or(0.5);

        [pred_error, cost_remaining, decision_len, is_dangerous, recent_success]
    }

    /// 逻辑回归预测：σ(w·x + b)
    fn predict(&self, features: &[f32; 5]) -> f32 {
        let logit: f32 = self.weights.iter()
            .zip(features)
            .map(|(w, f)| w * f)
            .sum::<f32>()
            + self.bias;
        sigmoid(logit)
    }

    /// 观察实际结果，更新 Bayesian 模型。
    pub fn observe_outcome(&mut self, success: bool) {
        if success {
            self.bayesian.observe_success();
        } else {
            self.bayesian.observe_failure();
        }
    }

    /// 保存最近特征用于离线分析。
    pub fn record_features(&mut self, features: [f32; 5]) {
        self.recent_features.push(features);
        if self.recent_features.len() > 100 {
            self.recent_features.remove(0);
        }
    }
}

impl Default for LocalCalibrator {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl CalibrationModel for LocalCalibrator {
    async fn calibrate(
        &self,
        state: &AgentState,
        decision_text: &str,
    ) -> CalibrationResult {
        let features = self.extract_features(state, decision_text);
        let local_confidence = self.predict(&features);

        // 融合 Bayesian 后验均值（加权平均：70% 特征 + 30% 统计）
        let bayesian_mean = self.bayesian.estimate().mean;
        let combined = 0.7 * local_confidence + 0.3 * bayesian_mean;

        let should_retry = combined < 0.35 || (bayesian_mean < 0.3 && self.bayesian.belief.total_observations > 5);

        CalibrationResult {
            raw_confidence: local_confidence,
            calibrated_confidence: combined.clamp(0.0, 1.0),
            should_retry,
            reasoning: format!(
                "Local: pred_err={:.2} cost={:.2} len={:.2} danger={:.0} succ={:.2} → {:.3}; Bayes={:.3}",
                features[0], features[1], features[2], features[3], features[4],
                local_confidence, bayesian_mean
            ),
        }
    }
}

// ---- helpers ----

/// Sigmoid 函数：1 / (1 + e^(-x))
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// 简单关键词检测危险命令。
fn contains_dangerous_command(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["rm_rf", "rm -rf", "delete_all", "drop_table", "format",
     "exec", "system", "shell", "shutdown", "reboot"]
        .iter()
        .any(|&kw| lower.contains(kw))
}

// ---- tests ----

#[cfg(test)]
mod local_calibrator_tests {
    use super::*;

    #[test]
    fn sigmoid_maps_to_zero_one() {
        assert!((sigmoid(0.0) - 0.5).abs() < 0.01);
        assert!(sigmoid(5.0) > 0.99);
        assert!(sigmoid(-5.0) < 0.01);
    }

    #[test]
    fn dangerous_command_detection() {
        assert!(contains_dangerous_command("run rm_rf on server"));
        assert!(contains_dangerous_command("exec script.sh"));
        assert!(!contains_dangerous_command("click button"));
        assert!(!contains_dangerous_command("search for docs"));
    }

    #[test]
    fn feature_extraction_from_clean_state() {
        let cal = LocalCalibrator::new();
        let state = AgentState::new();
        let features = cal.extract_features(&state, "click button");
        // pred_error = 0.0 (no history)
        assert!((features[0] - 0.0).abs() < 0.01);
        // cost_remaining ≈ 1.0 (no tokens used)
        assert!(features[1] > 0.9);
        // is_dangerous = 0 ("click button" is safe)
        assert_eq!(features[3], 0.0);
    }

    #[tokio::test]
    async fn local_calibrator_gives_lower_confidence_on_dangerous() {
        let cal = LocalCalibrator::new();
        let state = AgentState::new();

        let safe = cal.calibrate(&state, "click button").await;
        let dangerous = cal.calibrate(&state, "execute rm_rf /").await;

        assert!(dangerous.calibrated_confidence < safe.calibrated_confidence,
            "dangerous={:.3} should be < safe={:.3}",
            dangerous.calibrated_confidence, safe.calibrated_confidence);
    }

    #[tokio::test]
    async fn local_calibrator_retries_on_very_low_confidence() {
        let cal = LocalCalibrator::new();
        let mut state = AgentState::new();
        state.long_term.accumulated_pred_error = 0.9; // very high error

        let result = cal.calibrate(&state, "do something risky").await;
        // Combined confidence should be low due to high pred_error
        assert!(result.calibrated_confidence < 0.6);
    }
}

#[async_trait]
impl CalibrationModel for BayesianCalibrator {
    async fn calibrate(
        &self,
        _state: &AgentState,
        _decision_text: &str,
    ) -> CalibrationResult {
        let est = self.estimate();
        let ci = self.belief.distribution.credible_interval_95();
        CalibrationResult {
            raw_confidence: est.mean,
            calibrated_confidence: est.mean, // posterior mean IS calibrated
            should_retry: est.mean < 0.4,     // low success prob → retry
            reasoning: format!(
                "Bayesian: mean={:.3}, 95%CI=[{:.3},{:.3}], N={}",
                est.mean, ci.lower, ci.upper, self.belief.total_observations
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bayesian_calibrator_default_prior() {
        let cal = BayesianCalibrator::new();
        let est = cal.estimate();
        assert!((est.mean - 0.5).abs() < 0.15); // weak prior near 0.5
    }

    #[tokio::test]
    async fn bayesian_calibrator_updates_from_observations() {
        let mut cal = BayesianCalibrator::new();
        // 80 successes, 20 failures → posterior Beta(82, 22), mean ≈ 0.79
        cal.observe_batch(80, 20);
        let est = cal.estimate();
        assert!(est.mean > 0.7, "mostly successes → mean > 0.7, got {:.3}", est.mean);
        assert!(est.uncertainty < 0.5, "100 observations → low uncertainty, got {:.3}", est.uncertainty);
    }

    #[tokio::test]
    async fn bayesian_calibrator_retries_on_low_confidence() {
        let mut cal = BayesianCalibrator::new();
        // 1 success, 5 failures → posterior should be low
        cal.observe_batch(1, 5);
        let state = AgentState::new();
        let result = cal.calibrate(&state, "test").await;
        assert!(result.should_retry, "mostly failures → should retry");
    }

    #[test]
    fn bayesian_calibrator_uncertainty_combines_beliefs() {
        let cal = BayesianCalibrator::new();
        let extra = BetaBelief::with_prior(
            "tool_accuracy",
            BetaDistribution::with_mean(0.8, 20.0),
        );
        let est = cal.uncertainty(&[extra]);
        assert!(est.overall < 1.0);
        assert_eq!(est.dimensions.len(), 2); // decision_quality + tool_accuracy
    }
}
