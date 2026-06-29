//! FlowGraph —— 决策管道配置 + 节点编排

use serde::{Deserialize, Serialize};

/// 管道阶段标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    /// 感知：输入 → ContextDescriptor
    Perception,
    /// 记忆：ContextDescriptor → 相关记忆
    Memory,
    /// 推理：State + Memory → Decision
    Reasoning,
    /// 执行：Decision → ExecutionResult
    Execution,
    /// 验证（安全检查，可选）
    Validate,
}

/// 管道中的单条边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEdge {
    pub from: Stage,
    pub to: Stage,
}

/// FlowGraph 配置 —— 定义决策管道的拓扑结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowConfig {
    /// 管道包含的阶段
    pub stages: Vec<Stage>,
    /// 阶段间连接
    pub edges: Vec<FlowEdge>,
    /// 是否包含验证回边（reasoning → validate → reasoning）
    pub validation_loop: bool,
}

impl Default for FlowConfig {
    fn default() -> Self {
        Self::standard()
    }
}

impl FlowConfig {
    /// 标准 P→M→R→E 管道
    pub fn standard() -> Self {
        Self {
            stages: vec![
                Stage::Perception,
                Stage::Memory,
                Stage::Reasoning,
                Stage::Execution,
            ],
            edges: vec![
                FlowEdge { from: Stage::Perception, to: Stage::Memory },
                FlowEdge { from: Stage::Memory, to: Stage::Reasoning },
                FlowEdge { from: Stage::Reasoning, to: Stage::Execution },
            ],
            validation_loop: false,
        }
    }

    /// 高安全管道：标准 + reasoning → validate 回边
    pub fn high_security() -> Self {
        let mut config = Self::standard();
        config.stages.push(Stage::Validate);
        config.edges.push(FlowEdge { from: Stage::Reasoning, to: Stage::Validate });
        config.edges.push(FlowEdge { from: Stage::Validate, to: Stage::Reasoning });
        config.validation_loop = true;
        config
    }

    /// 从自定义 stages 构建
    pub fn custom(stages: Vec<Stage>, edges: Vec<FlowEdge>) -> Self {
        Self {
            stages,
            edges,
            validation_loop: false,
        }
    }

    /// 添加一个阶段和它的入边
    pub fn add_stage(&mut self, stage: Stage, from: Stage) {
        if !self.stages.contains(&stage) {
            self.stages.push(stage);
        }
        self.edges.push(FlowEdge { from, to: stage });
    }

    /// 获取某个阶段的所有前置阶段
    pub fn predecessors(&self, stage: Stage) -> Vec<Stage> {
        self.edges
            .iter()
            .filter(|e| e.to == stage)
            .map(|e| e.from)
            .collect()
    }
}

/// FlowGraph —— 决策管道的运行时表示
pub struct FlowGraph {
    pub config: FlowConfig,
}

impl FlowGraph {
    /// 从配置创建
    pub fn new(config: FlowConfig) -> Self {
        Self { config }
    }

    /// 标准 P→M→R→E 管道
    pub fn standard() -> Self {
        Self::new(FlowConfig::standard())
    }

    /// 高安全管道
    pub fn high_security() -> Self {
        Self::new(FlowConfig::high_security())
    }

    /// 运行时动态添加边（热更新）
    pub fn add_edge_dynamic(&mut self, from: Stage, to: Stage) {
        self.config.edges.push(FlowEdge { from, to });
        if !self.config.stages.contains(&to) {
            self.config.stages.push(to);
        }
    }

    /// 获取管道的阶段数
    pub fn stage_count(&self) -> usize {
        self.config.stages.len()
    }

    /// 获取管道的边数
    pub fn edge_count(&self) -> usize {
        self.config.edges.len()
    }

    /// 是否包含验证回边
    pub fn has_validation(&self) -> bool {
        self.config.validation_loop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_config_has_four_stages() {
        let config = FlowConfig::standard();
        assert_eq!(config.stages.len(), 4);
        assert!(config.stages.contains(&Stage::Perception));
        assert!(config.stages.contains(&Stage::Execution));
    }

    #[test]
    fn high_security_has_validation() {
        let config = FlowConfig::high_security();
        assert!(config.validation_loop);
        assert!(config.stages.contains(&Stage::Validate));
    }

    #[test]
    fn predecessors_correct() {
        let config = FlowConfig::standard();
        let preds = config.predecessors(Stage::Reasoning);
        assert_eq!(preds.len(), 1);
        assert_eq!(preds[0], Stage::Memory);
    }

    #[test]
    fn add_edge_dynamic_works() {
        let mut graph = FlowGraph::standard();
        graph.add_edge_dynamic(Stage::Execution, Stage::Perception);
        assert!(graph.config.stages.contains(&Stage::Perception));
        assert_eq!(graph.edge_count(), 4);
    }
}
