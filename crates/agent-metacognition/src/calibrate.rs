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

impl Default for BayesianCalibrator {
    fn default() -> Self { Self::new() }
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
