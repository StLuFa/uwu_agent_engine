//! # agent-character
//!
//! 人格 —— 底层不可变的核心价值观（安全锚点）+ 上层可调整的决策偏好。
//!
//! ## 两层结构
//!
//! | 层 | 可变性 | 内容 | 示例 |
//! |---|---|---|---|
//! | 核心价值观 | **不可变** | 硬/软约束 | "不泄露隐私"、"不执行破坏性命令" |
//! | 决策偏好 | 可调整 | 工具偏好/风险容忍度/策略 | "优先搜索"、"步骤式输出" |
//!
//! ## 三层约束体系
//!
//! ```text
//! Character.core_values（HardConstraint） → 决策层约束
//! Persona.relationships                   → 社交层约束
//! GuardLayer（硬闸门）                    → 执行层约束
//! ```

mod preferences;
mod values;

pub use preferences::{OutputStyle, Preferences, UncertaintyStrategy};
pub use values::{CoreValue, ValueEnforcement, ValueViolation};

use agent_types_core::Action;
use serde::{Deserialize, Serialize};

/// 人格 —— 核心价值观 + 可调偏好
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    /// 核心价值观（构造后不可变）
    pub core_values: Vec<CoreValue>,
    /// 决策偏好（可调整）
    pub preferences: Preferences,
}

impl Character {
    /// 检查动作是否违反核心价值观（HardConstraint 级别）
    pub fn check_core_values(&self, action: &Action) -> Result<(), ValueViolation> {
        for v in &self.core_values {
            if v.enforcement == ValueEnforcement::HardConstraint && v.violates(action) {
                return Err(ValueViolation {
                    value: v.name.clone(),
                    reason: v.description.clone(),
                });
            }
        }
        Ok(())
    }

    /// 生成上下文注入字符串（供推理时注入 system prompt）
    pub fn to_context_injection(&self) -> CharacterContext {
        CharacterContext {
            output_style: self.preferences.output_style,
            uncertainty_strategy: self.preferences.uncertainty_strategy,
            risk_tolerance: self.preferences.risk_tolerance,
        }
    }
}

/// CharacterContext —— 注入推理上下文的精简表示
#[derive(Debug, Clone)]
pub struct CharacterContext {
    pub output_style: OutputStyle,
    pub uncertainty_strategy: UncertaintyStrategy,
    pub risk_tolerance: f32,
}
