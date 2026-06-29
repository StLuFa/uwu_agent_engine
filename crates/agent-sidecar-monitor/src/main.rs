//! Monitor —— 独立监控进程。
//!
//! 异常检测引擎 + 定期生成 MetacognitiveReport。
//!
//! 生产环境：agent-mesh 元认知事件 → pred_error → channel → run_monitor()

use agent_sidecar_monitor::{run_monitor, AnomalyDetector};
use tokio::sync::mpsc;

/// 通过 channel 发送 demo pred_error 数据进行监控演示。
#[tokio::main]
async fn main() {
    println!("[monitor] starting...");

    let detector = AnomalyDetector::new(50, 0.2);
    let (tx, rx) = mpsc::channel::<f32>(64);
    let mut report_rx = run_monitor(detector, rx, 10);

    // 发送 demo 数据
    for i in 0..60 {
        let val = if i < 30 { 0.2 } else { 0.6 };
        tx.send(val).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    drop(tx); // 关闭发送端 → monitor 退出

    // 收集报告
    while let Ok(report) = report_rx.try_recv() {
        println!(
            "[monitor] received report: drift={}, anomalies={}",
            report.drift_detected, report.anomaly_count
        );
    }

    println!("[monitor] done");
}
