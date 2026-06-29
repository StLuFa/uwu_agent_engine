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

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use agent_types_core::{Action, ActionParams};

    #[test]
    fn hard_constraint_blocks_forbidden_action() {
        let character = Character {
            core_values: vec![CoreValue::no_destructive_actions()],
            preferences: Preferences::default(),
        };
        let action = Action::new("delete_all", ActionParams::new());
        assert!(character.check_core_values(&action).is_err());
    }

    #[test]
    fn hard_constraint_allows_safe_action() {
        let character = Character {
            core_values: vec![CoreValue::no_destructive_actions()],
            preferences: Preferences::default(),
        };
        let action = Action::new("click", ActionParams::new().with("target", "btn"));
        assert!(character.check_core_values(&action).is_ok());
    }

    #[test]
    fn soft_guideline_does_not_block() {
        let character = Character {
            core_values: vec![CoreValue::new(
                "prefer-short",
                "prefer short responses",
                ValueEnforcement::SoftGuideline,
            )],
            preferences: Preferences::default(),
        };
        // SoftGuideline with no forbidden keywords → never blocks
        let action = Action::new("write_long_text", ActionParams::new());
        assert!(character.check_core_values(&action).is_ok());
    }

    #[test]
    fn privacy_first_blocks_leak() {
        let character = Character {
            core_values: vec![CoreValue::privacy_first()],
            preferences: Preferences::default(),
        };
        let action = Action::new("leak_data", ActionParams::new());
        assert!(character.check_core_values(&action).is_err());
    }

    #[test]
    fn honesty_first_blocks_fabricate() {
        let character = Character {
            core_values: vec![CoreValue::honesty_first()],
            preferences: Preferences::default(),
        };
        let action = Action::new("fabricate_report", ActionParams::new());
        assert!(character.check_core_values(&action).is_err());
    }

    #[test]
    fn context_injection_contains_preferences() {
        let character = Character {
            core_values: vec![],
            preferences: Preferences::new()
                .with_output_style(OutputStyle::StepByStep)
                .with_uncertainty_strategy(UncertaintyStrategy::AskUserFirst)
                .with_risk_tolerance(0.3),
        };
        let ctx = character.to_context_injection();
        assert_eq!(ctx.output_style, OutputStyle::StepByStep);
        assert_eq!(ctx.uncertainty_strategy, UncertaintyStrategy::AskUserFirst);
        assert!((ctx.risk_tolerance - 0.3).abs() < 0.001);
    }
}
