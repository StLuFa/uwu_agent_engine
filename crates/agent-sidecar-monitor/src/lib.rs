//! Monitor library — 可嵌入的异常检测与元认知报告引擎。
//!
//! 滑动窗口异常检测 + 定期生成 MetacognitiveReport。
//!
//! ```ignore
//! use agent_sidecar_monitor::{AnomalyDetector, run_monitor};
//! let detector = AnomalyDetector::new(50, 0.2);
//! let (tx, rx) = tokio::sync::mpsc::channel(64);
//! let report_rx = run_monitor(detector, rx, 10);
//! tx.send(0.3).await.unwrap();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 元认知报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetacognitiveReport {
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
pub struct AnomalyDetector {
    window: Vec<f32>,
    window_size: usize,
    drift_threshold: f32,
    baseline: f32,
}

impl AnomalyDetector {
    pub fn new(window_size: usize, drift_threshold: f32) -> Self {
        Self {
            window: Vec::with_capacity(window_size),
            window_size,
            drift_threshold,
            baseline: 0.5,
        }
    }

    /// 喂入一个预测误差值
    pub fn feed(&mut self, value: f32) {
        self.window.push(value);
        if self.window.len() > self.window_size {
            self.window.remove(0);
        }
    }

    /// 当前窗口均值
    pub fn current_mean(&self) -> f32 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.window.iter().sum::<f32>() / self.window.len() as f32
    }

    /// EMA 更新基线
    pub fn update_baseline(&mut self) {
        let mean = self.current_mean();
        self.baseline = 0.9 * self.baseline + 0.1 * mean;
    }

    /// 检测概念漂移（窗口为空时不报告）
    pub fn is_drifting(&self) -> bool {
        if self.window.is_empty() {
            return false;
        }
        (self.baseline - self.current_mean()).abs() > self.drift_threshold
    }

    /// 异常值计数
    pub fn anomaly_count(&self) -> u64 {
        self.window
            .iter()
            .filter(|&&v| v > self.baseline + self.drift_threshold)
            .count() as u64
    }

    /// 生成元认知报告
    pub fn generate_report(&self, window_secs: u64, total: u64) -> MetacognitiveReport {
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
                format!(
                    "DRIFT DETECTED: baseline={:.3}, current={:.3}",
                    self.baseline, mean
                )
            } else {
                format!(
                    "stable: baseline={:.3}, current={:.3}",
                    self.baseline, mean
                )
            },
        }
    }
}

/// 运行监控循环：从 channel 消费 pred_error，定期生成报告。
///
/// 返回报告接收端，调用方可通过 `report_rx.recv().await` 获取报告。
pub fn run_monitor(
    mut detector: AnomalyDetector,
    mut rx: tokio::sync::mpsc::Receiver<f32>,
    report_interval_secs: u64,
) -> tokio::sync::mpsc::Receiver<MetacognitiveReport> {
    let (report_tx, report_rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        use tokio::time::{Duration, Instant};
        let mut last_report = Instant::now();
        let mut total_events = 0u64;

        println!("[monitor] running (channel-based, awaiting events)...");
        loop {
            tokio::select! {
                // Wait for next pred_error, or timeout to generate report
                recv_result = rx.recv() => {
                    match recv_result {
                        Some(pred_error) => {
                            detector.feed(pred_error);
                            total_events += 1;
                        }
                        None => {
                            println!("[monitor] channel closed, total events: {total_events}");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // Tick: check if it's time to generate a report
                }
            }

            if last_report.elapsed().as_secs() >= report_interval_secs {
                detector.update_baseline();
                let report = detector.generate_report(report_interval_secs, total_events);
                println!(
                    "[monitor] report: {} (drift={})",
                    report.summary, report.drift_detected
                );
                if report_tx.send(report).await.is_err() {
                    eprintln!("[monitor] report receiver dropped, stopping");
                    break;
                }
                last_report = Instant::now();
            }
        }
    });
    report_rx
}

/// Run the monitor loop over NATS/JetStream (requires `nats` feature).
///
/// Subscribes to `agent.{correlation_id}.monitoring` via JetStream,
/// deserializes pred_error values, feeds them to the anomaly detector,
/// and generates periodic reports.
#[cfg(feature = "nats")]
pub async fn run_monitor_with_nats(
    mut detector: AnomalyDetector,
    nats_url: &str,
    correlation_id: &str,
    report_interval_secs: u64,
) -> Result<(), uwu_nats_bridge::SubscribeError> {
    use uwu_nats_bridge::{NatsConfig, NatsSubscriber};

    let cfg = NatsConfig::for_sidecar(nats_url, "monitor");
    let mut sub = NatsSubscriber::connect(cfg, correlation_id).await?;

    let (report_tx, mut report_rx) = tokio::sync::mpsc::channel::<MetacognitiveReport>(16);

    println!(
        "[monitor] connected to NATS, listening on agent.{}.monitoring",
        correlation_id
    );

    tokio::spawn(async move {
        use tokio::time::{Duration, Instant};
        let mut last_report = Instant::now();
        let mut total_events = 0u64;

        loop {
            tokio::select! {
                env = sub.recv_monitoring() => {
                    match env {
                        Some(env) => {
                            // pred_error is serialized as a JSON float in the payload_bytes
                            if let Ok(pred_error) = serde_json::from_slice::<f32>(&env.payload_bytes) {
                                detector.feed(pred_error);
                                total_events += 1;
                            }
                        }
                        None => {
                            println!("[monitor] NATS subscription ended, total events: {total_events}");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // Tick — check if it's time to generate a report
                }
            }

            if last_report.elapsed().as_secs() >= report_interval_secs {
                detector.update_baseline();
                let report = detector.generate_report(report_interval_secs, total_events);
                println!(
                    "[monitor] report: {} (drift={})",
                    report.summary, report.drift_detected
                );
                if report_tx.send(report).await.is_err() {
                    eprintln!("[monitor] report receiver dropped, stopping");
                    break;
                }
                last_report = Instant::now();
            }
        }
    });

    // Collect and print reports on the main task.
    while let Some(report) = report_rx.recv().await {
        println!(
            "[monitor] received report: drift={}, anomalies={}, avg_pred_error={:.3}",
            report.drift_detected, report.anomaly_count, report.avg_pred_error
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_detector_defaults() {
        let d = AnomalyDetector::new(50, 0.2);
        assert!((d.current_mean() - 0.0).abs() < 0.001);
        assert!(!d.is_drifting());
    }

    #[test]
    fn feed_and_mean() {
        let mut d = AnomalyDetector::new(10, 0.2);
        for _ in 0..5 {
            d.feed(0.3);
        }
        let mean = d.current_mean();
        assert!((mean - 0.3).abs() < 0.001);
    }

    #[test]
    fn detects_drift() {
        let mut d = AnomalyDetector::new(10, 0.2);
        // Establish baseline
        for _ in 0..10 {
            d.feed(0.3);
        }
        d.update_baseline();
        // Introduce drift
        for _ in 0..10 {
            d.feed(0.8);
        }
        assert!(d.is_drifting());
    }
}
