//! Metacognition + MetacognitiveAssessment + 三信号融合

use crate::anomaly::AnomalyDetector;
use crate::calibrate::{CalibrationModel, CalibrationResult};
use crate::history::{CalibrationHistory, CalibrationRecord};
use crate::tts::{classify_tts, TTSSignal};
use crate::MetaAction;
use agent_state::long::BudgetConsumed;
use agent_state::AgentState;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

/// 元分数权重 —— 三信号融合系数
///
/// ```text
/// meta_score = w1 × verifier + w2 × (1 - pred_error) + w3 × cost_remaining
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaScoreWeights {
    /// verifier 权重（默认 0.5）
    pub verifier: f32,
    /// pred_error 权重（默认 0.3）
    pub pred_error: f32,
    /// cost_remaining 权重（默认 0.2）
    pub cost_remaining: f32,
}

impl Default for MetaScoreWeights {
    fn default() -> Self {
        Self {
            verifier: 0.5,
            pred_error: 0.3,
            cost_remaining: 0.2,
        }
    }
}

/// 元认知评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetacognitiveAssessment {
    /// 校准结果
    pub calibration: CalibrationResult,
    /// 三信号融合分数 [0.0, 1.0]
    pub meta_score: f32,
    /// 是否感知到未知（meta_score < 0.4）
    pub knows_unknown: bool,
    /// 是否检测到概念漂移
    pub concept_drifting: bool,
    /// 是否预算耗尽
    pub budget_exhausted: bool,
    /// 建议的元动作
    pub suggested_action: MetaAction,
}

/// 元认知 —— 三信号在线自校准 + TTS 渐进式预算控制
pub struct Metacognition {
    /// 校准模型（本地小模型或 mock）
    calibration_model: Box<dyn CalibrationModel>,
    /// 校准历史
    calibration_history: CalibrationHistory,
    /// 异常检测器
    anomaly_detector: AnomalyDetector,
    /// 三信号权重
    weights: MetaScoreWeights,
    /// token 预算上限
    token_budget: u64,
    /// 时间预算上限
    time_budget: Duration,
    /// 重试次数上限
    retry_budget: u32,
}

impl Metacognition {
    /// 创建元认知实例
    pub fn new(
        calibration_model: Box<dyn CalibrationModel>,
        token_budget: u64,
        time_budget: Duration,
        retry_budget: u32,
    ) -> Self {
        Self {
            calibration_model,
            calibration_history: CalibrationHistory::default(),
            anomaly_detector: AnomalyDetector::default(),
            weights: MetaScoreWeights::default(),
            token_budget,
            time_budget,
            retry_budget,
        }
    }

    /// 设置三信号权重
    pub fn with_weights(mut self, weights: MetaScoreWeights) -> Self {
        self.weights = weights;
        self
    }

    // =======================================================================
    // compute_cost_remaining() —— 委托给 BudgetConsumed
    // =======================================================================

    /// 计算剩余预算比例
    ///
    /// 委托给 `BudgetConsumed::cost_remaining_fraction()`。
    pub fn compute_cost_remaining(&self, consumed: &BudgetConsumed) -> f32 {
        consumed.cost_remaining_fraction(self.token_budget, self.time_budget, self.retry_budget)
    }

    // =======================================================================
    // tts_signal() —— TTS 分级
    // =======================================================================

    /// TTS（Time To Stop）信号
    ///
    /// 根据 cost_remaining 分为四级：
    /// - cost > 0.5 → Normal
    /// - 0.2 < cost ≤ 0.5 → Degraded（禁用 ToT）
    /// - 0.05 < cost ≤ 0.2 → Urgent（仅 Reaction + 直接回答）
    /// - cost ≤ 0.05 → Abort
    pub fn tts_signal(&self, consumed: &BudgetConsumed) -> TTSSignal {
        let c = self.compute_cost_remaining(consumed);
        classify_tts(c)
    }

    // =======================================================================
    // evaluate() —— 三信号融合 + 模式检测 → MetaAction
    // =======================================================================

    /// 三信号融合评估
    ///
    /// 三路信号：
    /// - **verifier**: 校准模型对当前决策的置信度
    /// - **pred_error**: 1.0 - accumulated_pred_error（来自 State 推演沙盒，零 LLM call）
    /// - **cost_remaining**: 剩余预算比例（纯计算，零 LLM call）
    ///
    /// 同步消费 `MidTermWS.recent_pattern` 检测循环模式。
    pub async fn evaluate(
        &self,
        state: &AgentState,
        decision_text: &str,
    ) -> MetacognitiveAssessment {
        // 1. 校准模型评估
        let cal = self.calibration_model.calibrate(state, decision_text).await;
        let verifier = cal.calibrated_confidence;

        // 2. 预测误差信号（零 LLM call）
        let pred = 1.0 - state.long_term.accumulated_pred_error;

        // 3. 剩余预算信号（零 LLM call）
        let cost = self.compute_cost_remaining(&state.long_term.budget_consumed);

        // 三信号融合
        let meta = self.weights.verifier * verifier
            + self.weights.pred_error * pred
            + self.weights.cost_remaining * cost;

        // 消费 MidTermWS.recent_pattern：元认知"看见模式"
        let pattern_loop = state
            .mid_term
            .recent_pattern
            .as_ref()
            .map(|p| p.is_loop_detected())
            .unwrap_or(false);
        let low_success = state
            .mid_term
            .recent_pattern
            .as_ref()
            .map(|p| p.is_failure_loop(0.3, 5))
            .unwrap_or(false);

        let knows_unknown = meta < 0.4;
        let drifting = self.anomaly_detector.detect_drift();
        let budget_exhausted = cost <= 0.05;

        // 确定建议动作
        let suggested_action = if budget_exhausted {
            MetaAction::AbortOnBudget
        } else if pattern_loop || low_success {
            MetaAction::SwitchStrategy
        } else if knows_unknown {
            MetaAction::RequestClarification
        } else if drifting {
            MetaAction::SwitchStrategy
        } else if cal.should_retry {
            MetaAction::RetryDecision
        } else {
            MetaAction::Proceed
        };

        MetacognitiveAssessment {
            calibration: cal,
            meta_score: meta.clamp(0.0, 1.0),
            knows_unknown,
            concept_drifting: drifting,
            budget_exhausted,
            suggested_action,
        }
    }

    // =======================================================================
    // calibrate_with_outcome() —— 在线校准
    // =======================================================================

    /// 根据实际结果进行在线校准
    ///
    /// - 调用 `state.update_pred_error(actual)` 更新 EMA 预测误差
    /// - 追加 `CalibrationRecord` 到历史
    /// - 更新异常检测器
    pub fn calibrate_with_outcome(
        &mut self,
        state: &mut AgentState,
        actual: &AgentState,
        calibration: &CalibrationResult,
        meta_score: f32,
    ) {
        // 更新 JEPA 预测误差
        state.update_pred_error(actual);

        // 追加校准记录
        self.calibration_history.push(CalibrationRecord {
            predicted_state_id: state.state_id.0.clone(),
            actual_state_id: Some(actual.state_id.0.clone()),
            calibration: calibration.clone(),
            meta_score,
            timestamp: Utc::now(),
        });

        // 更新异常检测器
        self.anomaly_detector.update(&self.calibration_history);
    }

    // =======================================================================
    // 访问器
    // =======================================================================

    pub fn calibration_history(&self) -> &CalibrationHistory {
        &self.calibration_history
    }

    pub fn weights(&self) -> &MetaScoreWeights {
        &self.weights
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibrate::CalibrationResult;
    use async_trait::async_trait;

    /// Mock 校准模型 —— 返回固定置信度
    struct MockCalibrationModel {
        confidence: f32,
        should_retry: bool,
    }

    #[async_trait]
    impl CalibrationModel for MockCalibrationModel {
        async fn calibrate(
            &self,
            _state: &AgentState,
            _decision_text: &str,
        ) -> CalibrationResult {
            CalibrationResult {
                raw_confidence: self.confidence,
                calibrated_confidence: self.confidence,
                should_retry: self.should_retry,
                reasoning: "mock".into(),
            }
        }
    }

    fn make_metacognition() -> Metacognition {
        Metacognition::new(
            Box::new(MockCalibrationModel {
                confidence: 0.8,
                should_retry: false,
            }),
            10_000,
            Duration::seconds(120),
            5,
        )
    }

    #[tokio::test]
    async fn evaluate_proceed_with_high_confidence() {
        let mc = make_metacognition();
        let state = AgentState::new();

        let assessment = mc.evaluate(&state, "click button").await;

        assert_eq!(assessment.suggested_action, MetaAction::Proceed);
        // meta = 0.5*0.8 + 0.3*1.0 + 0.2*1.0 = 0.4 + 0.3 + 0.2 = 0.9
        assert!((assessment.meta_score - 0.9).abs() < 0.01);
        assert!(!assessment.knows_unknown);
        assert!(!assessment.budget_exhausted);
    }

    #[tokio::test]
    async fn evaluate_knows_unknown_when_low_meta() {
        // With high pred_error, verifier low → meta drops below 0.4
        let mc = Metacognition::new(
            Box::new(MockCalibrationModel {
                confidence: 0.2,
                should_retry: false,
            }),
            10_000,
            Duration::seconds(120),
            5,
        );
        let mut state = AgentState::new();
        state.long_term.accumulated_pred_error = 0.8;

        let assessment = mc.evaluate(&state, "click").await;
        // meta = 0.5*0.2 + 0.3*(1-0.8) + 0.2*1.0 = 0.1 + 0.06 + 0.2 = 0.36 < 0.4
        assert!(assessment.knows_unknown);
        assert_eq!(assessment.suggested_action, MetaAction::RequestClarification);
    }

    #[tokio::test]
    async fn evaluate_abort_on_budget_exhausted() {
        let mc = make_metacognition();
        let mut state = AgentState::new();
        // Exhaust budget by using all tokens
        state.long_term.budget_consumed.tokens_used = 10_000;

        let assessment = mc.evaluate(&state, "click").await;

        assert_eq!(assessment.suggested_action, MetaAction::AbortOnBudget);
        assert!(assessment.budget_exhausted);
    }

    #[tokio::test]
    async fn evaluate_switch_strategy_on_loop_detected() {
        let mc = make_metacognition();
        let mut state = AgentState::new();
        state.mid_term.recent_pattern = Some(agent_state::mid::InteractionPattern {
            recent_success_rate: 0.2,
            detected_pattern: Some("loop_detected".into()),
            pattern_since_step: 6,
        });

        let assessment = mc.evaluate(&state, "click").await;

        assert_eq!(assessment.suggested_action, MetaAction::SwitchStrategy);
    }

    #[tokio::test]
    async fn evaluate_retry_when_should_retry() {
        let mc = Metacognition::new(
            Box::new(MockCalibrationModel {
                confidence: 0.8,
                should_retry: true,
            }),
            10_000,
            Duration::seconds(120),
            5,
        );
        let state = AgentState::new();

        let assessment = mc.evaluate(&state, "click").await;

        assert_eq!(assessment.suggested_action, MetaAction::RetryDecision);
    }

    #[test]
    fn tts_signal_maps_cost_correctly() {
        let mc = make_metacognition();

        let mut consumed = BudgetConsumed::new();
        consumed.tokens_used = 0;
        assert_eq!(mc.tts_signal(&consumed), TTSSignal::Normal);

        consumed.tokens_used = 6_000; // 4000/10000 = 0.4
        assert_eq!(mc.tts_signal(&consumed), TTSSignal::Degraded { disable_tot: true });

        consumed.tokens_used = 9_000; // 1000/10000 = 0.1
        assert_eq!(
            mc.tts_signal(&consumed),
            TTSSignal::Urgent {
                allow_reaction: true,
                allow_new_tool: false
            }
        );

        consumed.tokens_used = 10_000; // 0/10000 = 0.0
        assert_eq!(mc.tts_signal(&consumed), TTSSignal::Abort);
    }

    #[test]
    fn calibrate_with_outcome_updates_pred_error() {
        let mut mc = make_metacognition();
        let mut state = AgentState::new();
        state.mid_term.known_facts.push(agent_state::mid::Fact::new("x", "1", 1.0));

        let mut actual = state.clone();
        actual.mid_term.known_facts.push(agent_state::mid::Fact::new("y", "2", 1.0));

        let cal = CalibrationResult {
            raw_confidence: 0.8,
            calibrated_confidence: 0.8,
            should_retry: false,
            reasoning: "ok".into(),
        };

        mc.calibrate_with_outcome(&mut state, &actual, &cal, 0.9);

        // pred_error should have been updated (no longer 0.0)
        assert!(state.long_term.accumulated_pred_error > 0.0);
        // History should have one record
        assert_eq!(mc.calibration_history().len(), 1);
    }

    #[test]
    fn calibration_history_ring_buffer() {
        let mut history = CalibrationHistory::new(10);
        for i in 0..15 {
            history.push(CalibrationRecord {
                predicted_state_id: format!("s{i}"),
                actual_state_id: None,
                calibration: CalibrationResult {
                    raw_confidence: 0.5,
                    calibrated_confidence: 0.5,
                    should_retry: false,
                    reasoning: String::new(),
                },
                meta_score: 0.5,
                timestamp: Utc::now(),
            });
        }
        // Should cap at 10
        assert_eq!(history.len(), 10);
        // Oldest record should be s5 (records s0-s4 were evicted)
        let recent = history.recent(1);
        assert_eq!(recent[0].predicted_state_id, "s14");
    }

    #[test]
    fn meta_score_weights_default() {
        let w = MetaScoreWeights::default();
        assert!((w.verifier - 0.5).abs() < 0.001);
        assert!((w.pred_error - 0.3).abs() < 0.001);
        assert!((w.cost_remaining - 0.2).abs() < 0.001);
    }
}
