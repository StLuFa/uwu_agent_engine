//! Monitor —— 独立监控进程。
//!
//! 异常检测引擎 + 定期生成 MetacognitiveReport。
//!
//! # 运行模式
//!
//! - **Demo 模式**（默认）：mock channel 发送测试 pred_error 数据
//! - **NATS 模式**（`--features nats`）：连接 NATS/JetStream，消费真实 monitoring 通道
//!
//! ```bash
//! # Demo 模式
//! cargo run -p agent-sidecar-monitor
//!
//! # NATS 生产模式
//! cargo run -p agent-sidecar-monitor --features nats -- --nats nats://localhost:4222 --session "*"
//! ```

use agent_sidecar_monitor::{run_monitor, AnomalyDetector};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let nats_mode = args.iter().any(|a| a == "--nats");

    #[cfg(feature = "nats")]
    if nats_mode {
        let nats_url = args
            .iter()
            .position(|a| a == "--nats")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str())
            .unwrap_or("nats://localhost:4222");

        let session = args
            .iter()
            .position(|a| a == "--session")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str())
            .unwrap_or("*");

        println!("[monitor] NATS mode: {nats_url}, session={session}");
        let detector = AnomalyDetector::new(50, 0.2);
        match agent_sidecar_monitor::run_monitor_with_nats(detector, nats_url, session, 10).await {
            Ok(()) => println!("[monitor] done"),
            Err(e) => eprintln!("[monitor] error: {e}"),
        }
        return;
    }

    #[cfg(not(feature = "nats"))]
    if nats_mode {
        eprintln!("[monitor] NATS feature not enabled. Rebuild with: cargo build --features nats");
        return;
    }

    // ---- Demo mode (default) ----
    println!("[monitor] demo mode (use --nats <url> for production)...");

    let detector = AnomalyDetector::new(50, 0.2);
    let (tx, rx) = mpsc::channel::<f32>(64);
    let mut report_rx = run_monitor(detector, rx, 10);

    // Send demo data.
    for i in 0..60 {
        let val = if i < 30 { 0.2 } else { 0.6 };
        tx.send(val).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    drop(tx);

    // Collect reports.
    while let Ok(report) = report_rx.try_recv() {
        println!(
            "[monitor] received report: drift={}, anomalies={}",
            report.drift_detected, report.anomaly_count
        );
    }

    println!("[monitor] done");
}
