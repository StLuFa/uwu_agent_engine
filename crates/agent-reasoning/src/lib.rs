//! # agent-reasoning
//!
//! 推理域 —— 消费 AgentState + fork() 推演沙盒 + Tree-of-Thought beam search。
//!
//! 作为 visual_script NodeDefinition 注册：`"reasoning.decide"`（Impure + Async）
//!
//! ## 推理策略（根据 TTSSignal 切换）
//!
//! | TTSSignal | 策略 |
//! |---|---|
//! | Normal | ToT beam search（多候选推演 + 剪枝） |
//! | Degraded | 单步推理（禁用 ToT） |
//! | Urgent | 直接回答（禁止新工具调用） |
//! | Abort | 终止 |

mod reasoner;
mod sandbox;
mod strategies;
mod tot;

pub use reasoner::{Decision, Reasoner};
pub use sandbox::SandboxEvaluator;
pub use strategies::ReasoningStrategy;
pub use tot::{ToTExplorer, ToTConfig};

use agent_state::AgentState;
use agent_types_core::Action;

/// 推理输入
#[derive(Debug, Clone)]
pub struct ReasoningInput {
    pub goal: String,
    pub state_snapshot: AgentState,
    pub persona_context: Option<String>,
    pub character_context: Option<String>,
}

/// 推理输出
#[derive(Debug, Clone)]
pub struct ReasoningOutput {
    pub decision: Decision,
    pub state_delta: agent_state::StateDiff,
    pub tokens_used: u64,
}
