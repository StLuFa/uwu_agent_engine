//! # agent-core
//!
//! Agent 核心 —— 会话管理 + FlowGraph(基于 uwu_visual_script) + FlowEngine + CapabilityRegistry。
//!
//! 这是整个 Agent 引擎的顶层聚合 crate。它不实现新能力，而是将其他 crate
//! 组装成可用的 Agent 实例。
//!
//! ## 模块
//!
//! - `flow` — FlowGraph 领域包装层（基于 uwu_visual_script）
//! - `engine` — FlowEngine 主循环执行器
//! - `capability` — CapabilityRegistry 动态能力注册

pub mod capability;
pub mod engine;
pub mod flow;

pub use capability::CapabilityRegistry;
pub use engine::FlowEngine;
pub use flow::{FlowGraph, FlowConfig};

use agent_session::Session;

/// Agent 顶层门面
pub struct Agent {
    pub session: Session,
}

impl Agent {
    /// 处理用户输入
    pub async fn process(&mut self, input: &str) -> String {
        let result = self.session.process_turn(input).await;
        result.output.content
    }
}
