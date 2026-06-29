//! Monitor —— 独立监控进程。
//!
//! 异常检测引擎 + 定期生成 MetacognitiveReport。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// 元认知报告
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetacognitiveReport {
    pub report_id: String,
    pub generated_at: DateTime<Utc>,
    pub window_secs: u64,
    pub total_events: u64,
    pub anomaly_count: u64,
    pub drift_detected: bool,
    pub avg_pred_error: f32,
    pub summary: String,
}

/// 滑动窗口异常检测器
struct AnomalyDetector {
    window: Vec<f32>,
    window_size: usize,
    drift_threshold: f32,
    baseline: f32,
}

impl AnomalyDetector {
    fn new(window_size: usize, drift_threshold: f32) -> Self {
        Self {
            window: Vec::with_capacity(window_size),
            window_size,
            drift_threshold,
            baseline: 0.5,
        }
    }

    fn feed(&mut self, value: f32) {
        self.window.push(value);
        if self.window.len() > self.window_size {
            self.window.remove(0);
        }
    }

    fn current_mean(&self) -> f32 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.window.iter().sum::<f32>() / self.window.len() as f32
    }

    fn update_baseline(&mut self) {
        let mean = self.current_mean();
        self.baseline = 0.9 * self.baseline + 0.1 * mean;
    }

    fn is_drifting(&self) -> bool {
        (self.baseline - self.current_mean()).abs() > self.drift_threshold
    }

    fn anomaly_count(&self) -> u64 {
        self.window
            .iter()
            .filter(|&&v| v > self.baseline + self.drift_threshold)
            .count() as u64
    }

    fn generate_report(&self, window_secs: u64, total: u64) -> MetacognitiveReport {
        let mean = self.current_mean();
        let drifting = self.is_drifting();
        MetacognitiveReport {
            report_id: uuid::Uuid::new_v4().to_string(),
            generated_at: Utc::now(),
            window_secs,
            total_events: total,
            anomaly_count: self.anomaly_count(),
            drift_detected: drifting,
            avg_pred_error: mean,
            summary: if drifting {
                format!("DRIFT DETECTED: baseline={:.3}, current={:.3}", self.baseline, mean)
            } else {
                format!("stable: baseline={:.3}, current={:.3}", self.baseline, mean)
            },
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[monitor] starting...");

    let mut detector = AnomalyDetector::new(50, 0.2);
    let report_interval_secs = 10u64;
    let mut last_report = Instant::now();
    let mut total_events = 0u64;

    // Main loop: poll for monitoring events
    loop {
        // Simulate receiving a monitoring event (production: from agent-mesh monitoring channel)
        let pred_error = mock_pred_error(total_events);
        detector.feed(pred_error);
        total_events += 1;

        // Generate report periodically
        if last_report.elapsed().as_secs() >= report_interval_secs {
            detector.update_baseline();
            let report = detector.generate_report(report_interval_secs, total_events);
            println!(
                "[monitor] report: {} (drift={})",
                report.summary, report.drift_detected
            );
            last_report = Instant::now();
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        if total_events >= 100 {
            println!("[monitor] processed {} events, shutting down", total_events);
            break;
        }
    }

    Ok(())
}

/// Mock pred_error values (production: from Metacognition `meta_score` events)
fn mock_pred_error(n: u64) -> f32 {
    // Simulate stable then degrading
    let base = if n < 50 { 0.2 } else { 0.6 };
    base + (n as f32 * 0.001).sin() * 0.05
}
