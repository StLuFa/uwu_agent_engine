//! Consolidator —— 独立巩固进程。
//!
//! 消费 agent-mesh 的 consolidation 通道：
//! 1. 反序列化 Episode（TypeRegistry 校验）
//! 2. LearnTrigger 评估是否触发学习
//! 3. Guard egress 博弈（McpRemote 需 Guard 放行）
//! 4. UnifiedMemory 持久化（consolidate episode）

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    tracing::info!("[consolidator] starting...");

    // TODO: 阶段 8 实现
    // let cfg = ConsolidatorConfig::from_env();
    // let mesh = connect_mesh(&cfg).await?;
    // let memory = UnifiedMemory::connect(&cfg.memory).await?;
    // let guard = GuardLayer::from_config(&cfg.guard);
    // let learn_trigger = LearnTrigger::from_config(&cfg.learning);
    //
    // while let Some(env) = rx.recv_consolidation().await {
    //     let episode: Episode = env.deserialize_payload()?;
    //     let decision = learn_trigger.evaluate(&episode);
    //     match decision {
    //         LearnDecision::ExtractSkill { target: SkillTarget::McpRemote { .. }, .. } => {
    //             if guard.check_egress(&target).await.is_ok() {
    //                 memory.consolidate(episode).await?;
    //             }
    //         }
    //         _ => { memory.consolidate(episode).await?; }
    //     }
    // }

    tracing::info!("[consolidator] shutting down");
    Ok(())
}
