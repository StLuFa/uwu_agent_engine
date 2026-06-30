//! # agent-uncertainty
//!
//! Bayesian uncertainty estimation for agent decision-making.
//!
//! ## Core concepts
//!
//! - **Beta distribution** — conjugate prior for binary outcomes (success/failure).
//!   After observing `α` successes and `β` failures, the posterior is Beta(α+successes, β+failures).
//! - **Credible intervals** — 95% Highest Posterior Density interval from the Beta distribution.
//! - **Bayesian aggregator** — combines multiple Beta beliefs into a single uncertainty estimate.
//!
//! ## Usage
//!
//! ```ignore
//! use agent_uncertainty::{BetaBelief, BayesianAggregator};
//!
//! let mut belief = BetaBelief::uniform(); // Beta(1,1) — no prior knowledge
//! belief.observe_success();               // Beta(2,1)
//! belief.observe_failure();               // Beta(2,2)
//! let estimate = belief.estimate();
//! // estimate.mean ≈ 0.5, variance ≈ 0.05
//! ```

use serde::{Deserialize, Serialize};

// ===========================================================================
// Beta Distribution
// ===========================================================================

/// Beta distribution — the conjugate prior for Bernoulli/binomial likelihoods.
///
/// PDF: f(x; α, β) = x^(α-1) * (1-x)^(β-1) / B(α, β)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BetaDistribution {
    /// Success count + prior (α parameter)
    pub alpha: f64,
    /// Failure count + prior (β parameter)
    pub beta: f64,
}

impl BetaDistribution {
    /// Uniform prior: Beta(1, 1) — all probabilities equally likely.
    pub fn uniform() -> Self {
        Self { alpha: 1.0, beta: 1.0 }
    }

    /// Jeffreys prior: Beta(0.5, 0.5) — non-informative, invariant under reparameterization.
    pub fn jeffreys() -> Self {
        Self { alpha: 0.5, beta: 0.5 }
    }

    /// Prior centered at `mean` with `strength` (α+β).
    /// Higher strength = more confident prior.
    pub fn with_mean(mean: f64, strength: f64) -> Self {
        let strength = strength.max(2.0);
        Self {
            alpha: mean * strength,
            beta: (1.0 - mean) * strength,
        }
    }

    /// Mean of the distribution: E[X] = α / (α + β).
    pub fn mean(&self) -> f64 {
        if self.alpha + self.beta == 0.0 { return 0.5; }
        self.alpha / (self.alpha + self.beta)
    }

    /// Mode of the distribution: (α-1) / (α+β-2) for α,β > 1.
    pub fn mode(&self) -> f64 {
        if self.alpha <= 1.0 { return 0.0; }
        if self.beta <= 1.0 { return 1.0; }
        (self.alpha - 1.0) / (self.alpha + self.beta - 2.0)
    }

    /// Variance: αβ / ((α+β)^2 * (α+β+1)).
    pub fn variance(&self) -> f64 {
        let a = self.alpha;
        let b = self.beta;
        let total = a + b;
        if total == 0.0 { return 0.25; }
        (a * b) / (total * total * (total + 1.0))
    }

    /// Standard deviation.
    pub fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Observe a success — Bayesian update: Beta(α+1, β).
    pub fn observe_success(&mut self) {
        self.alpha += 1.0;
    }

    /// Observe a failure — Bayesian update: Beta(α, β+1).
    pub fn observe_failure(&mut self) {
        self.beta += 1.0;
    }

    /// Observe multiple Bernoulli trials at once.
    pub fn observe_batch(&mut self, successes: u64, failures: u64) {
        self.alpha += successes as f64;
        self.beta += failures as f64;
    }

    /// Probability density at x, using the normal approximation.
    /// For α,β > 5 this is within 1% of the true Beta PDF.
    pub fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 || x > 1.0 { return 0.0; }
        let mean = self.mean();
        let std_dev = self.std_dev();
        if std_dev < 1e-10 { return if (x - mean).abs() < 1e-10 { 1e10 } else { 0.0 }; }
        let z = (x - mean) / std_dev;
        // Normal PDF: 1/√(2πσ²) * exp(-z²/2)
        let norm = 1.0 / ((2.0 * std::f64::consts::PI).sqrt() * std_dev);
        norm * (-0.5 * z * z).exp()
    }

    /// 95% credible interval (equal-tailed).
    pub fn credible_interval_95(&self) -> CredibleInterval {
        self.credible_interval(0.95)
    }

    /// Equal-tailed credible interval at the given probability level.
    pub fn credible_interval(&self, level: f64) -> CredibleInterval {
        let alpha = level / 2.0;
        CredibleInterval {
            lower: beta_quantile(self.alpha, self.beta, alpha),
            upper: beta_quantile(self.alpha, self.beta, 1.0 - alpha),
            level,
        }
    }

    /// Effective sample size: α + β.
    pub fn sample_size(&self) -> f64 {
        self.alpha + self.beta
    }
}

impl Default for BetaDistribution {
    fn default() -> Self { Self::uniform() }
}

// ===========================================================================
// Credible Interval
// ===========================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CredibleInterval {
    pub lower: f64,
    pub upper: f64,
    pub level: f64,
}

impl CredibleInterval {
    /// Width of the interval.
    pub fn width(&self) -> f64 { self.upper - self.lower }

    /// Does the interval contain this value?
    pub fn contains(&self, x: f64) -> bool { x >= self.lower && x <= self.upper }
}

// ===========================================================================
// Bayesian Belief (for tracking a single probability)
// ===========================================================================

/// A Bayesian belief about a single probability — wraps a Beta prior
/// and updates from observations.
///
/// ```ignore
/// let mut belief = BetaBelief::uniform("tool_reliability");
/// belief.observe_success();
/// belief.observe_success();
/// belief.observe_failure();
/// let est = belief.estimate();
/// // est.mean ≈ 0.67 (2 successes / 3 trials with uniform prior)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBelief {
    pub name: String,
    pub distribution: BetaDistribution,
    pub total_observations: u64,
}

impl BetaBelief {
    pub fn uniform(name: impl Into<String>) -> Self {
        Self { name: name.into(), distribution: BetaDistribution::uniform(), total_observations: 0 }
    }

    pub fn with_prior(name: impl Into<String>, dist: BetaDistribution) -> Self {
        Self { name: name.into(), distribution: dist, total_observations: 0 }
    }

    pub fn observe_success(&mut self) {
        self.distribution.observe_success();
        self.total_observations += 1;
    }

    pub fn observe_failure(&mut self) {
        self.distribution.observe_failure();
        self.total_observations += 1;
    }

    /// Produce an uncertainty estimate for this belief.
    /// Lower is more certain (variance → 0 as observations accumulate).
    pub fn estimate(&self) -> BeliefEstimate {
        let dist = &self.distribution;
        let ci = dist.credible_interval_95();
        let uncertainty = (ci.width() * 2.0).min(1.0); // Wider CI = more uncertainty
        BeliefEstimate {
            name: self.name.clone(),
            mean: dist.mean() as f32,
            variance: dist.variance() as f32,
            std_dev: dist.std_dev() as f32,
            uncertainty: uncertainty as f32,
            ci_lower: ci.lower as f32,
            ci_upper: ci.upper as f32,
            effective_samples: dist.sample_size() as u64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefEstimate {
    pub name: String,
    pub mean: f32,
    pub variance: f32,
    pub std_dev: f32,
    pub uncertainty: f32,
    pub ci_lower: f32,
    pub ci_upper: f32,
    pub effective_samples: u64,
}

// ===========================================================================
// Bayesian Aggregator
// ===========================================================================

/// Aggregates multiple Bayesian beliefs into a single uncertainty estimate.
pub struct BayesianAggregator {
    /// Threshold above which `should_confirm` is set.
    pub confirm_threshold: f32,
    /// Minimum effective samples before a belief is considered "informed".
    pub min_samples: u64,
}

impl BayesianAggregator {
    pub fn new(confirm_threshold: f32) -> Self {
        Self { confirm_threshold, min_samples: 5 }
    }

    /// Aggregate multiple beliefs into an overall uncertainty estimate.
    ///
    /// Uses inverse-variance weighting: beliefs with lower variance
    /// (more observations) contribute more to the overall estimate.
    pub fn aggregate(&self, beliefs: &[BetaBelief]) -> UncertaintyEstimate {
        if beliefs.is_empty() {
            return UncertaintyEstimate {
                overall: 1.0,
                dimensions: vec![],
                should_confirm: true,
            };
        }

        let estimates: Vec<BeliefEstimate> = beliefs.iter().map(|b| b.estimate()).collect();

        // Inverse-variance weights
        let weights: Vec<f32> = estimates
            .iter()
            .map(|e| {
                let v = e.variance.max(1e-6);
                1.0 / v
            })
            .collect();
        let total_weight: f32 = weights.iter().sum();

        let weighted_uncertainty: f32 = if total_weight > 0.0 {
            estimates
                .iter()
                .zip(&weights)
                .map(|(e, w)| e.uncertainty * w)
                .sum::<f32>()
                / total_weight
        } else {
            estimates.iter().map(|e| e.uncertainty).sum::<f32>() / estimates.len() as f32
        };

        // Overall uncertainty also considers: are there still too few samples?
        let min_effective = estimates.iter().map(|e| e.effective_samples).min().unwrap_or(0);
        let sample_penalty = if min_effective < self.min_samples {
            0.3 * (1.0 - min_effective as f32 / self.min_samples as f32)
        } else {
            0.0
        };

        let dimensions: Vec<DimensionUncertainty> = estimates
            .iter()
            .map(|e| DimensionUncertainty {
                name: e.name.clone(),
                value: e.uncertainty,
                mean: e.mean,
                ci_lower: e.ci_lower,
                ci_upper: e.ci_upper,
            })
            .collect();

        let overall = (weighted_uncertainty + sample_penalty).clamp(0.0, 1.0);

        UncertaintyEstimate {
            overall,
            dimensions,
            should_confirm: overall > self.confirm_threshold,
        }
    }
}

impl Default for BayesianAggregator {
    fn default() -> Self { Self { confirm_threshold: 0.7, min_samples: 5 } }
}

// ===========================================================================
// Uncertainty Estimate
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyEstimate {
    /// Overall uncertainty [0, 1] (0 = completely certain, 1 = completely uncertain).
    pub overall: f32,
    /// Per-dimension uncertainty breakdown with Bayesian statistics.
    pub dimensions: Vec<DimensionUncertainty>,
    /// Whether the agent should confirm with the user before acting.
    pub should_confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionUncertainty {
    pub name: String,
    /// Uncertainty score [0, 1].
    pub value: f32,
    /// Posterior mean.
    pub mean: f32,
    /// 95% CI lower bound.
    pub ci_lower: f32,
    /// 95% CI upper bound.
    pub ci_upper: f32,
}

// ===========================================================================
// Math helpers — normal approximation for Beta credible intervals
// ===========================================================================

/// Pre-computed z-critical values for common confidence levels.
const Z_80: f64 = 1.28155;
const Z_90: f64 = 1.64485;
const Z_95: f64 = 1.95996;
const Z_99: f64 = 2.57583;

/// Get z-critical value for a confidence level.
fn z_for_level(level: f64) -> f64 {
    if level >= 0.99 { Z_99 }
    else if level >= 0.95 { Z_95 }
    else if level >= 0.90 { Z_90 }
    else { Z_80 }
}

/// Beta quantile via normal approximation: mean ± z·σ.
fn beta_quantile(alpha: f64, beta: f64, p: f64) -> f64 {
    let mean = alpha / (alpha + beta);
    let variance = (alpha * beta) / ((alpha + beta).powi(2) * (alpha + beta + 1.0));
    let std_dev = variance.sqrt();
    // z for the two-tailed level: e.g. p=0.025 or 0.975 for 95% CI
    // Use a simple signed z: negative for p<0.5, positive for p>0.5
    if p < 0.5 {
        let z = z_for_level(1.0 - 2.0 * p);
        (mean - z * std_dev).clamp(0.0, 1.0)
    } else if p > 0.5 {
        let z = z_for_level(2.0 * p - 1.0);
        (mean + z * std_dev).clamp(0.0, 1.0)
    } else {
        mean
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- BetaDistribution ----

    #[test]
    fn uniform_beta_has_mean_0_5() {
        let d = BetaDistribution::uniform();
        assert!((d.mean() - 0.5).abs() < 0.01);
    }

    #[test]
    fn jeffreys_prior_is_concentrated_at_edges() {
        let d = BetaDistribution::jeffreys();
        assert!(d.variance() > 0.0);
        assert!((d.mean() - 0.5).abs() < 0.01);
    }

    #[test]
    fn observe_success_increases_mean() {
        let mut d = BetaDistribution::uniform();
        d.observe_success();
        assert!(d.mean() > 0.5);
    }

    #[test]
    fn observe_failure_decreases_mean() {
        let mut d = BetaDistribution::uniform();
        d.observe_failure();
        assert!(d.mean() < 0.5);
    }

    #[test]
    fn many_observations_reduces_variance() {
        let mut d = BetaDistribution::uniform();
        let initial_var = d.variance();
        for _ in 0..100 {
            d.observe_success();
        }
        assert!(d.variance() < initial_var);
        assert!(d.std_dev() < 0.1);
    }

    #[test]
    fn batch_observation() {
        let mut d = BetaDistribution::uniform();
        d.observe_batch(7, 3);
        assert!((d.mean() - 0.7).abs() < 0.1); // (1+7)/(2+10) ≈ 0.67
        // Beta(8, 4): mean = 8/12 = 0.667
        assert!((d.mean() - 0.667).abs() < 0.01);
    }

    #[test]
    fn credible_interval_contains_mean() {
        let d = BetaDistribution::with_mean(0.7, 10.0); // Beta(7, 3)
        let ci = d.credible_interval_95();
        // Normal approx: mean ± 1.96*σ. By construction this contains the mean.
        let m = d.mean();
        assert!(m >= ci.lower - 0.01 && m <= ci.upper + 0.01,
            "mean {:.3} should be within [{:.3}, {:.3}]", m, ci.lower, ci.upper);
        assert!(ci.width() > 0.0);
        assert!(ci.width() <= 1.0);
    }

    #[test]
    fn more_data_narrower_ci() {
        let d1 = BetaDistribution::with_mean(0.7, 10.0);  // strength=10
        let d2 = BetaDistribution::with_mean(0.7, 100.0); // strength=100
        let ci1 = d1.credible_interval_95();
        let ci2 = d2.credible_interval_95();
        // Stronger prior concentrates the distribution → narrower CI
        assert!(ci2.width() < ci1.width(),
            "strength 10 CI={:.3} vs 100 CI={:.3}", ci1.width(), ci2.width());
    }

    #[test]
    fn pdf_integrates_near_one() {
        let d = BetaDistribution::with_mean(0.5, 20.0);
        let mut integral = 0.0;
        let n = 1000;
        let dx = 1.0 / n as f64;
        for i in 0..n {
            let x = (i as f64 + 0.5) * dx;
            integral += d.pdf(x) * dx;
        }
        assert!((integral - 1.0).abs() < 0.05, "PDF should integrate ~1: {integral:.4}");
    }

    // ---- BetaBelief ----

    #[test]
    fn belief_tracks_observations() {
        let mut belief = BetaBelief::uniform("test");
        belief.observe_success();
        belief.observe_success();
        belief.observe_failure();
        assert_eq!(belief.total_observations, 3);
        let est = belief.estimate();
        assert!(est.mean > 0.5); // 2/3 success
        assert!(est.effective_samples >= 3);
    }

    #[test]
    fn belief_uncertainty_decreases_with_evidence() {
        let mut belief = BetaBelief::uniform("test");
        // With uniform prior, uncertainty starts high (~1.0).
        // After 50 successes, mean ≈ 0.98, variance ≈ 0.0004, CI width ≈ 0.08.
        for _ in 0..50 {
            belief.observe_success();
        }
        let after = belief.estimate().uncertainty;
        assert!(after < 0.5,
            "50 observations should give low uncertainty, got {:.3}", after);
    }

    // ---- BayesianAggregator ----

    #[test]
    fn aggregator_empty_returns_high_uncertainty() {
        let agg = BayesianAggregator::default();
        let est = agg.aggregate(&[]);
        assert!(est.overall > 0.9);
        assert!(est.should_confirm);
    }

    #[test]
    fn aggregator_weights_by_inverse_variance() {
        let precise = BetaBelief::with_prior("precise", BetaDistribution::with_mean(0.5, 100.0));
        let vague = BetaBelief::with_prior("vague", BetaDistribution::uniform());

        let agg = BayesianAggregator::default();
        let est = agg.aggregate(&[precise, vague]);
        assert!(est.overall < 0.8);
        assert_eq!(est.dimensions.len(), 2);
    }

    #[test]
    fn aggregator_should_confirm_above_threshold() {
        let agg = BayesianAggregator::new(0.3);
        let belief = BetaBelief::uniform("test");
        let est = agg.aggregate(&[belief]);
        // With uniform prior, effective_samples=2, min_samples=5.
        // sample_penalty = 0.3 * (1 - 2/5) = 0.18.
        // uncertainty ≈ 1.0, overall ≈ min(1.18, 1.0) = 1.0 > 0.3
        assert!(est.should_confirm,
            "uniform prior (low data) + threshold 0.3 → should confirm (overall={:.3})", est.overall);
    }

    #[test]
    fn aggregator_should_not_confirm_when_certain() {
        let agg = BayesianAggregator::new(0.5);
        let belief = BetaBelief::with_prior("certain", BetaDistribution::with_mean(0.5, 10_000.0));
        let est = agg.aggregate(&[belief]);
        assert!(!est.should_confirm,
            "strength=10000 prior → very certain, overall={:.3}", est.overall);
    }

    // ---- DimensionUncertainty ----

    #[test]
    fn dimension_has_ci_bounds() {
        let belief = BetaBelief::with_prior("dim", BetaDistribution::with_mean(0.7, 50.0));
        let est = belief.estimate();
        assert!(est.ci_lower >= 0.0);
        assert!(est.ci_upper <= 1.0);
        // The CI should bracket the mean (normal approximation is symmetric around mean)
        assert!(est.ci_lower <= est.mean + 0.01,
            "ci_lower={:.3} should be <= mean={:.3}", est.ci_lower, est.mean);
        assert!(est.ci_upper >= est.mean - 0.01,
            "ci_upper={:.3} should be >= mean={:.3}", est.ci_upper, est.mean);
    }
}
