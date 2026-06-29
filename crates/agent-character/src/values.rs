//! CoreValue + ValueEnforcement + ValueViolation

use agent_types_core::Action;
use serde::{Deserialize, Serialize};

/// 价值观约束级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueEnforcement {
    /// 硬约束 —— 违反则阻断执行
    HardConstraint,
    /// 软指导 —— 警告但不阻断
    SoftGuideline,
}

/// 核心价值观
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreValue {
    /// 价值观名称
    pub name: String,
    /// 详细描述
    pub description: String,
    /// 约束级别
    pub enforcement: ValueEnforcement,
    /// 违反检测的关键词列表（检查 action.command / params）
    pub forbidden_keywords: Vec<String>,
}

impl CoreValue {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        enforcement: ValueEnforcement,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            enforcement,
            forbidden_keywords: Vec::new(),
        }
    }

    pub fn with_forbidden(mut self, keywords: Vec<String>) -> Self {
        self.forbidden_keywords = keywords;
        self
    }

    /// 检测动作是否违反此价值观
    pub fn violates(&self, action: &Action) -> bool {
        if self.forbidden_keywords.is_empty() {
            return false;
        }
        let command_lower = action.command.to_lowercase();
        self.forbidden_keywords
            .iter()
            .any(|kw| command_lower.contains(&kw.to_lowercase()))
    }
}

/// 价值观违反记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueViolation {
    /// 违反的价值观名称
    pub value: String,
    /// 违反原因
    pub reason: String,
}

// ===========================================================================
// 内置核心价值观预设
// ===========================================================================

impl CoreValue {
    /// 隐私优先：不泄露敏感信息
    pub fn privacy_first() -> Self {
        Self::new(
            "privacy-first",
            "不泄露用户隐私数据或敏感信息",
            ValueEnforcement::HardConstraint,
        )
        .with_forbidden(vec![
            "leak".into(),
            "expose".into(),
            "share_pii".into(),
            "dump".into(),
        ])
    }

    /// 诚实优先：不编造信息
    pub fn honesty_first() -> Self {
        Self::new(
            "honesty-first",
            "不编造事实、不伪造数据、不伪装成人类",
            ValueEnforcement::HardConstraint,
        )
        .with_forbidden(vec![
            "fabricate".into(),
            "pretend_human".into(),
            "forge".into(),
            "impersonate".into(),
        ])
    }

    /// 禁止破坏性操作
    pub fn no_destructive_actions() -> Self {
        Self::new(
            "no-destructive",
            "不执行破坏性命令（删除数据、修改生产配置等）",
            ValueEnforcement::HardConstraint,
        )
        .with_forbidden(vec![
            "delete_all".into(),
            "drop_table".into(),
            "rm_rf".into(),
            "format".into(),
            "shutdown".into(),
            "destroy".into(),
        ])
    }
}
