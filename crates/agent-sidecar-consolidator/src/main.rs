//! Consolidator —— 独立巩固进程。
//!
//! 消费 Episode → LearnTrigger 评估 → Guard 博弈 → Memory 持久化。
//!
//! # 运行模式
//!
//! - **Demo 模式**（默认）：mock channel 发送 5 个测试 Episode
//! - **NATS 模式**（`--features nats`）：连接 NATS/JetStream，消费真实 consolidation 通道
//!
//! ```bash
//! # Demo 模式
//! cargo run -p agent-sidecar-consolidator
//!
//! # NATS 生产模式
//! cargo run -p agent-sidecar-consolidator --features nats -- --nats nats://localhost:4222 --session "*"
//! ```

use agent_learning::Episode;
use agent_sidecar_consolidator::Consolidator;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    // Parse command-line args for NATS mode.
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

        println!("[consolidator] NATS mode: {nats_url}, session={session}");
        let mut consolidator = Consolidator::new();
        match consolidator.run_with_nats(nats_url, session).await {
            Ok(()) => println!("[consolidator] done"),
            Err(e) => eprintln!("[consolidator] error: {e}"),
        }
        return;
    }

    #[cfg(not(feature = "nats"))]
    if nats_mode {
        eprintln!("[consolidator] NATS feature not enabled. Rebuild with: cargo build --features nats");
        return;
    }

    // ---- Demo mode (default) ----
    println!("[consolidator] demo mode (use --nats <url> for production)...");

    let (tx, rx) = mpsc::channel::<Episode>(64);
    let mut consolidator = Consolidator::new();

    // Send demo episodes.
    for i in 0..5 {
        let ep = Episode {
            episode_id: format!("demo-ep-{i}"),
            session_id: "demo-session".into(),
            task_id: None,
            state_before: None,
            state_after: None,
            actions_taken: vec!["search".into(), "click".into()],
            outcome: if i < 4 {
                agent_learning::EpisodeOutcome::Success { confidence: 0.9 }
            } else {
                agent_learning::EpisodeOutcome::Failure {
                    error: "timeout".into(),
                }
            },
            timestamp: chrono::Utc::now(),
        };
        tx.send(ep).await.unwrap();
    }
    drop(tx);

    consolidator.run(rx).await;
    println!(
        "[consolidator] done, processed {} episodes",
        consolidator.episode_count()
    );
}
