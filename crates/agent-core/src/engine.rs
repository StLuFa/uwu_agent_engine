//! FlowEngine —— 主循环执行器

use crate::capability::CapabilityRegistry;
use crate::flow::{FlowGraph, Stage};
use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};

/// 流程执行上下文 —— 在管道各阶段间传递
#[derive(Debug, Clone)]
pub struct FlowContext {
    /// 原始输入
    pub raw_input: String,
    /// 当前 State
    pub state: AgentState,
    /// 感知输出
    pub context_description: Option<String>,
    /// 检索到的记忆
    pub retrieved_memories: Vec<String>,
    /// 推理决策
    pub decision: Option<Decision>,
    /// 执行结果
    pub execution_output: Option<String>,
    /// 已执行的阶段列表
    pub completed_stages: Vec<Stage>,
    /// 执行日志
    pub log: Vec<String>,
}

/// 流程引擎决策（简化版，不依赖 agent-reasoning）
#[derive(Debug, Clone)]
pub struct Decision {
    pub actions: Vec<Action>,
    pub reasoning: String,
}

impl FlowContext {
    pub fn new(raw_input: impl Into<String>, state: AgentState) -> Self {
        Self {
            raw_input: raw_input.into(),
            state,
            context_description: None,
            retrieved_memories: Vec::new(),
            decision: None,
            execution_output: None,
            completed_stages: Vec::new(),
            log: Vec::new(),
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
    }
}

/// FlowEngine —— 按 FlowGraph 拓扑执行 P→M→R→E 管道
pub struct FlowEngine {
    registry: CapabilityRegistry,
}

impl FlowEngine {
    pub fn new(registry: CapabilityRegistry) -> Self {
        Self { registry }
    }

    /// 执行一个完整的 FlowGraph 管道
    pub async fn run(
        &self,
        flow: &FlowGraph,
        raw_input: &str,
        state: &AgentState,
    ) -> FlowContext {
        let mut ctx = FlowContext::new(raw_input, state.clone());

        for stage in &flow.config.stages {
            match stage {
                Stage::Perception => {
                    self.execute_perception(&mut ctx).await;
                }
                Stage::Memory => {
                    self.execute_memory(&mut ctx).await;
                }
                Stage::Reasoning => {
                    self.execute_reasoning(&mut ctx).await;
                }
                Stage::Execution => {
                    self.execute_execution(&mut ctx).await;
                }
                Stage::Validate => {
                    self.execute_validate(&mut ctx).await;
                }
            }
            ctx.completed_stages.push(*stage);
        }

        ctx
    }

    async fn execute_perception(&self, ctx: &mut FlowContext) {
        ctx.log("perception: parsing input");

        // 如果有注册的 Perception 处理器 → 调用它们
        if self.registry.has_stage(Stage::Perception) {
            for handler in self.registry.get(Stage::Perception) {
                ctx.log(format!("perception: running {}", handler.name()));
            }
        }

        // 更新 context_description
        ctx.context_description = Some(format!("parsed: {}", ctx.raw_input));
        ctx.log("perception: done");
    }

    async fn execute_memory(&self, ctx: &mut FlowContext) {
        ctx.log("memory: retrieving relevant memories");

        if self.registry.has_stage(Stage::Memory) {
            for handler in self.registry.get(Stage::Memory) {
                ctx.log(format!("memory: running {}", handler.name()));
            }
        }

        // Mock: retrieve memories based on context
        ctx.retrieved_memories = vec![format!(
            "memory relevant to: {}",
            ctx.context_description.as_deref().unwrap_or("")
        )];
        ctx.log("memory: done");
    }

    async fn execute_reasoning(&self, ctx: &mut FlowContext) {
        ctx.log("reasoning: deciding action");

        if self.registry.has_stage(Stage::Reasoning) {
            for handler in self.registry.get(Stage::Reasoning) {
                ctx.log(format!("reasoning: running {}", handler.name()));
            }
        }

        // Mock: generate a decision
        ctx.decision = Some(Decision {
            actions: vec![Action::new(
                "respond",
                ActionParams::new().with("text", format!(
                    "Processed: {}",
                    ctx.raw_input
                )),
            )],
            reasoning: format!(
                "Based on input '{}' and {} memories",
                ctx.raw_input,
                ctx.retrieved_memories.len()
            ),
        });
        ctx.log("reasoning: done");
    }

    async fn execute_execution(&self, ctx: &mut FlowContext) {
        ctx.log("execution: running action");

        if self.registry.has_stage(Stage::Execution) {
            for handler in self.registry.get(Stage::Execution) {
                ctx.log(format!("execution: running {}", handler.name()));
            }
        }

        if let Some(ref decision) = ctx.decision {
            if let Some(action) = decision.actions.first() {
                ctx.execution_output = Some(format!("executed: {}", action.command));
            }
        }
        ctx.log("execution: done");
    }

    async fn execute_validate(&self, ctx: &mut FlowContext) {
        ctx.log("validate: checking action safety");
        if self.registry.has_stage(Stage::Validate) {
            for handler in self.registry.get(Stage::Validate) {
                ctx.log(format!("validate: running {}", handler.name()));
            }
        }
        ctx.log("validate: done");
    }

    /// 获取能力注册表引用
    pub fn registry(&self) -> &CapabilityRegistry {
        &self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn standard_pipeline_executes() {
        let registry = CapabilityRegistry::new();
        let engine = FlowEngine::new(registry);
        let flow = FlowGraph::standard();
        let state = AgentState::new();

        let ctx = engine.run(&flow, "hello world", &state).await;

        assert_eq!(ctx.completed_stages.len(), 4);
        assert!(ctx.context_description.is_some());
        assert!(!ctx.retrieved_memories.is_empty());
        assert!(ctx.decision.is_some());
        assert!(ctx.execution_output.is_some());
        assert!(!ctx.log.is_empty());
    }

    #[tokio::test]
    async fn high_security_pipeline_executes() {
        let registry = CapabilityRegistry::new();
        let engine = FlowEngine::new(registry);
        let flow = FlowGraph::high_security();
        let state = AgentState::new();

        let ctx = engine.run(&flow, "test", &state).await;

        assert!(ctx.completed_stages.contains(&Stage::Validate));
    }

    #[tokio::test]
    async fn engine_with_registered_handlers() {
        use crate::capability::CapabilityHandler;

        struct MockHandler {
            stage: Stage,
            name: String,
        }
        impl CapabilityHandler for MockHandler {
            fn stage(&self) -> Stage { self.stage }
            fn name(&self) -> &str { &self.name }
        }

        let mut registry = CapabilityRegistry::new();
        registry.register(Box::new(MockHandler {
            stage: Stage::Perception,
            name: "custom-perceiver".into(),
        }));

        let engine = FlowEngine::new(registry);
        let flow = FlowGraph::standard();
        let state = AgentState::new();

        let ctx = engine.run(&flow, "test", &state).await;
        assert!(ctx.log.iter().any(|l| l.contains("custom-perceiver")));
    }
}
