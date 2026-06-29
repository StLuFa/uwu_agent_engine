//! # agent-reaction
//!
//! 反射短路 —— 每步决策前的独立拦截器。
//!
//! 命中则短路跳过 LLM，省 30-50% token。
//! Reaction 层是 Agent 决策循环的第一道闸门：
//! 高频低智操作（弹窗关闭、限流重试、验证码检测）在此被拦截，
//! 不进入 FlowGraph → LLM 的昂贵路径。
//!
//! ## 内置规则
//!
//! - `PopupCloseRule` — 检测弹窗关闭按钮 → 自动点击
//! - `RateLimitRetryRule` — 检测 rate-limit → 等待重试
//! - `CaptchaDetectRule` — 检测验证码 → 请求人工介入
//! - `IdleTimeoutRule` — 连续 N 步无进展 → 重新评估目标

mod rules;
mod stats;

pub use rules::{PopupCloseRule, RateLimitRetryRule, CaptchaDetectRule, IdleTimeoutRule};
pub use stats::ReactionStats;

use agent_state::AgentState;
use agent_types_core::{Action};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};

/// 反应结果：命中（直接返回动作）或未命中（进入 FlowGraph）
pub enum Reaction {
    Hit(Action),
    Miss,
}

/// 反应规则 trait —— 每个规则独立实现 match + react
#[async_trait]
pub trait ReactionRule: Send + Sync {
    /// 检查当前状态是否匹配此规则
    fn matches(&self, state: &AgentState) -> bool;
    /// 规则命中后执行的动作
    async fn react(&self, state: &AgentState) -> Action;
}

/// 反应层 —— 持有规则列表，按注册顺序依次匹配
pub struct ReactionLayer {
    rules: Vec<Box<dyn ReactionRule + Send + Sync>>,
    stats: ReactionStats,
}

impl ReactionLayer {
    /// Builder 入口
    pub fn builder() -> ReactionLayerBuilder {
        ReactionLayerBuilder { rules: Vec::new() }
    }

    /// 拦截：顺序匹配规则，首个命中即短路返回 Hit
    pub async fn intercept(&self, state: &AgentState) -> Reaction {
        for rule in &self.rules {
            if rule.matches(state) {
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                return Reaction::Hit(rule.react(state).await);
            }
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        Reaction::Miss
    }

    /// 获取统计数据
    pub fn stats(&self) -> &ReactionStats {
        &self.stats
    }
}

/// ReactionLayer 构建器
pub struct ReactionLayerBuilder {
    rules: Vec<Box<dyn ReactionRule + Send + Sync>>,
}

impl ReactionLayerBuilder {
    pub fn add_rule<R: ReactionRule + 'static>(mut self, rule: R) -> Self {
        self.rules.push(Box::new(rule));
        self
    }

    pub fn build(self) -> ReactionLayer {
        ReactionLayer {
            rules: self.rules,
            stats: ReactionStats::default(),
        }
    }
}
