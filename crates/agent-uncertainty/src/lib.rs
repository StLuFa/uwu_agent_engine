//! # agent-uncertainty
//!
//! 贝叶斯不确定性 —— 不确定性聚合器，集成到主循环推理。
//!
//! 对 AgentState 中每个事实的置信度进行贝叶斯聚合，
//! 输出整体不确定性估计，影响 Metacognition 的决策阈值。

use serde::{Deserialize, Serialize};

/// 不确定性估计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyEstimate {
    /// 整体不确定性 [0, 1]（0 = 完全确定，1 = 完全不确定）
    pub overall: f32,
    /// 各维度不确定性分解
    pub dimensions: Vec<DimensionUncertainty>,
    /// 是否应该向用户确认
    pub should_confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionUncertainty {
    pub name: String,
    pub value: f32,
}

/// 不确定性聚合器
pub struct UncertaintyAggregator {
    confirm_threshold: f32,
}

impl UncertaintyAggregator {
    pub fn new(confirm_threshold: f32) -> Self {
        Self { confirm_threshold }
    }

    /// 聚合各维度不确定性
    pub fn aggregate(&self, dimensions: Vec<DimensionUncertainty>) -> UncertaintyEstimate {
        let count = dimensions.len().max(1) as f32;
        let overall = dimensions.iter().map(|d| d.value).sum::<f32>() / count;
        UncertaintyEstimate {
            overall,
            should_confirm: overall > self.confirm_threshold,
            dimensions,
        }
    }
}

impl Default for UncertaintyAggregator {
    fn default() -> Self {
        Self { confirm_threshold: 0.7 }
    }
}
