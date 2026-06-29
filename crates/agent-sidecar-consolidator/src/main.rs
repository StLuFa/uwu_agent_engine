//! Consolidator —— 独立巩固进程。
//!
//! 从 channel 消费 Episode → LearnTrigger 评估 → Guard 博弈 → Memory 持久化。
//!
//! 生产环境：agent-mesh 事件 → Episode → channel → Consolidator::run()

use agent_learning::Episode;
use agent_sidecar_consolidator::Consolidator;
use tokio::sync::mpsc;

/// 通过 channel 发送 episode 进行巩固演示。
#[tokio::main]
async fn main() {
    println!("[consolidator] starting...");

    let (tx, rx) = mpsc::channel::<Episode>(64);
    let mut consolidator = Consolidator::new();

    // 发送一些 demo episodes
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
    drop(tx); // 关闭发送端 → run() 退出

    consolidator.run(rx).await;
    println!(
        "[consolidator] done, processed {} episodes",
        consolidator.episode_count()
    );
}
