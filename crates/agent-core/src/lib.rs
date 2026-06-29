//! # agent-core
//!
//! Agent 核心 —— FlowGraph + FlowEngine + CapabilityRegistry。
//!
//! 这是整个 Agent 引擎的编排层。不实现新能力，将各能力域组装成可执行的决策管道。

pub mod capability;
pub mod engine;
pub mod flow;
#[cfg(feature = "visual-script")]
pub mod vs_nodes;

pub use capability::CapabilityRegistry;
pub use engine::{Decision, FlowContext, FlowEngine};
pub use flow::{FlowConfig, FlowEdge, FlowGraph, Stage};

// ===========================================================================
// 单元测试（集成）
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_pipeline_integration() {
        let registry = CapabilityRegistry::new();
        // In real usage, register actual Perceiver/Reasoner/Executor implementations
        let engine = FlowEngine::new(registry);
        let flow = FlowGraph::standard();
        let state = agent_state::AgentState::new();

        let ctx = engine.run(&flow, "user clicked submit button", &state).await;

        assert!(ctx.context_description.unwrap().contains("clicked"));
        assert!(!ctx.retrieved_memories.is_empty());
        assert!(ctx.decision.is_some());
        assert!(ctx.execution_output.unwrap().contains("respond"));
        assert_eq!(ctx.completed_stages.len(), 4);
    }

    #[test]
    fn standard_flow_has_all_stages() {
        let flow = FlowGraph::standard();
        assert_eq!(flow.stage_count(), 4);
    }

    #[test]
    fn high_security_has_five_stages() {
        let flow = FlowGraph::high_security();
        assert_eq!(flow.stage_count(), 5);
        assert!(flow.has_validation());
    }
}
