//! Preferences + OutputStyle + UncertaintyStrategy

use serde::{Deserialize, Serialize};

/// 不确定时的决策策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UncertaintyStrategy {
    /// 先搜索更多信息
    SearchFirst,
    /// 先问用户
    AskUserFirst,
    /// 最佳猜测 + 事后确认
    BestGuessAndConfirm,
}

/// 输出风格
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputStyle {
    /// 简洁
    Concise,
    /// 详细
    Detailed,
    /// 逐步推理
    StepByStep,
}

/// Agent 决策偏好（可调整）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    /// 工具偏好（按优先级排序的名称列表）
    pub tool_preference: Vec<String>,
    /// 风险容忍度 [0.0, 1.0]（0 = 极保守，1 = 激进）
    pub risk_tolerance: f32,
    /// 不确定时的策略
    pub uncertainty_strategy: UncertaintyStrategy,
    /// 输出风格
    pub output_style: OutputStyle,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            tool_preference: Vec::new(),
            risk_tolerance: 0.5,
            uncertainty_strategy: UncertaintyStrategy::SearchFirst,
            output_style: OutputStyle::Concise,
        }
    }
}

impl Preferences {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_risk_tolerance(mut self, tolerance: f32) -> Self {
        self.risk_tolerance = tolerance.clamp(0.0, 1.0);
        self
    }

    pub fn with_uncertainty_strategy(mut self, strategy: UncertaintyStrategy) -> Self {
        self.uncertainty_strategy = strategy;
        self
    }

    pub fn with_output_style(mut self, style: OutputStyle) -> Self {
        self.output_style = style;
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tool_preference = tools;
        self
    }
}
