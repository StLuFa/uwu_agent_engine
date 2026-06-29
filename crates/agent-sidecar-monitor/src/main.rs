//! Monitor —— 独立监控进程。
//!
//! 消费 agent-mesh 的 monitoring 通道：
//! 1. 异常检测引擎（Metacognition 漂移 + State 异常模式）
//! 2. 定期生成 MetacognitiveReport
//! 3. 告警输出（日志 / OpenTelemetry / Webhook）

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    tracing::info!("[monitor] starting...");

    // TODO: 阶段 8 实现
    // let cfg = MonitorConfig::from_env();
    // let mut rx = connect_monitoring(&cfg).await?;
    //
    // let mut anomaly_detector = AnomalyDetector::new(cfg.drift_window);
    // let mut last_report = Instant::now();
    //
    // while let Some(env) = rx.recv_monitoring().await {
    //     anomaly_detector.feed(&env);
    //
    //     if last_report.elapsed() >= cfg.min_interval {
    //         let report = anomaly_detector.generate_report();
    //         tracing::info!("[monitor] report: {:?}", report);
    //         last_report = Instant::now();
    //     }
    // }

    tracing::info!("[monitor] shutting down");
    Ok(())
}
