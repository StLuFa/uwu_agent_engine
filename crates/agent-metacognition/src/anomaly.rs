//! AnomalyDetector —— 滑动窗口概念漂移检测

use crate::history::CalibrationHistory;
use serde::{Deserialize, Serialize};

/// 异常检测器 —— 检测校准分数的趋势退化
///
/// 使用滑动窗口比较最近分数与历史基线。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetector {
    /// 滑动窗口大小（最近 N 条）
    window_size: usize,
    /// 历史基线置信度均值（初始 0.5）
    baseline_mean: f32,
    /// 漂移阈值：当前窗口均值低于基线超过此值 → 漂移
    drift_threshold: f32,
    /// 当前窗口均值
    current_window_mean: f32,
}

impl AnomalyDetector {
    /// 创建检测器
    ///
    /// - `window_size`: 窗口大小（默认 50）
    /// - `drift_threshold`: 漂移阈值（默认 0.2）
    pub fn new(window_size: usize, drift_threshold: f32) -> Self {
        Self {
            window_size: window_size.max(5),
            baseline_mean: 0.5,
            drift_threshold,
            current_window_mean: 0.5,
        }
    }

    /// 检测是否存在概念漂移
    ///
    /// 当前窗口均值低于基线超过 drift_threshold 时判定为漂移。
    pub fn detect_drift(&self) -> bool {
        (self.baseline_mean - self.current_window_mean) > self.drift_threshold
    }

    /// 更新窗口统计 —— 每次 calibrate_with_outcome 后调用
    ///
    /// 从 CalibrationHistory 获取最近窗口内的校准分数，计算均值。
    pub fn update(&mut self, history: &CalibrationHistory) {
        let recent = history.recent(self.window_size);
        if recent.is_empty() {
            return;
        }

        let sum: f32 = recent
            .iter()
            .map(|r| r.calibration.calibrated_confidence)
            .sum();
        let new_mean = sum / recent.len() as f32;

        // EMA 更新基线（给历史更多权重）
        self.baseline_mean = 0.9 * self.baseline_mean + 0.1 * new_mean;
        self.current_window_mean = new_mean;
    }

    /// 当前窗口均值
    pub fn current_window_mean(&self) -> f32 {
        self.current_window_mean
    }

    /// 历史基线均值
    pub fn baseline_mean(&self) -> f32 {
        self.baseline_mean
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new(50, 0.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibrate::CalibrationResult;
    use crate::history::{CalibrationHistory, CalibrationRecord};
    use chrono::Utc;

    fn make_record(confidence: f32, meta_score: f32) -> CalibrationRecord {
        CalibrationRecord {
            predicted_state_id: "test".into(),
            actual_state_id: None,
            calibration: CalibrationResult {
                raw_confidence: confidence,
                calibrated_confidence: confidence,
                should_retry: false,
                reasoning: "test".into(),
            },
            meta_score,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn no_drift_when_stable() {
        let mut history = CalibrationHistory::new(100);
        for _ in 0..60 {
            history.push(make_record(0.8, 0.7));
        }

        let mut detector = AnomalyDetector::new(50, 0.2);
        detector.update(&history);
        // Baseline should be near 0.8
        assert!(!detector.detect_drift());
    }

    #[test]
    fn detects_drift_when_confidence_drops() {
        let mut history = CalibrationHistory::new(100);

        // First 60 records at high confidence → establish baseline
        for _ in 0..60 {
            history.push(make_record(0.9, 0.8));
        }

        let mut detector = AnomalyDetector::new(50, 0.2);
        detector.update(&history);
        assert!(!detector.detect_drift());

        // Now add 50 records at low confidence → current window drops
        for _ in 0..50 {
            history.push(make_record(0.3, 0.3));
        }
        detector.update(&history);

        // Current window = 0.3, baseline ≈ 0.9*0.79 + 0.1*0.3 ≈ 0.74
        // Drift = 0.74 - 0.3 = 0.44 > 0.2 → true
        assert!(detector.detect_drift());
    }

    #[test]
    fn default_detector_values() {
        let d = AnomalyDetector::default();
        assert_eq!(d.current_window_mean(), 0.5);
        assert_eq!(d.baseline_mean(), 0.5);
    }
}
