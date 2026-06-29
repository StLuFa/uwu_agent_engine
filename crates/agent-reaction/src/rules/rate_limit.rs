//! RateLimitRetryRule —— 检测限流响应，自动等待重试

use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

use super::super::ReactionRule;

/// 限流关键词列表
const RATE_LIMIT_KEYWORDS: &[&str] = &[
    "429",
    "rate limit",
    "rate-limited",
    "too many requests",
    "too many request",
    "throttled",
    "retry after",
    "retry-after",
    "限流",
    "请求过于频繁",
    "稍后再试",
    "try again later",
    "slow down",
];

/// 检测 rate-limit 响应 → 等待后重试
pub struct RateLimitRetryRule;

#[async_trait]
impl ReactionRule for RateLimitRetryRule {
    fn matches(&self, state: &AgentState) -> bool {
        // 检查最近的观察结果
        if let Some(ref observation) = state.short_term.last_observation {
            let lower = observation.to_lowercase();
            return RATE_LIMIT_KEYWORDS
                .iter()
                .any(|kw| lower.contains(&kw.to_lowercase()));
        }
        // 也检查上下文描述
        let text = &state.short_term.current_context.description;
        if !text.is_empty() {
            let lower = text.to_lowercase();
            return RATE_LIMIT_KEYWORDS
                .iter()
                .any(|kw| lower.contains(&kw.to_lowercase()));
        }
        false
    }

    async fn react(&self, _state: &AgentState) -> Action {
        Action::new(
            "wait_retry",
            ActionParams::new().with("delay_ms", 5000),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_state::AgentState;

    #[test]
    fn matches_429_in_observation() {
        let mut state = AgentState::new();
        state.short_term.last_observation = Some("HTTP 429 Too Many Requests".into());
        assert!(RateLimitRetryRule.matches(&state));
    }

    #[test]
    fn matches_rate_limit_in_context() {
        let mut state = AgentState::new();
        state.short_term.current_context.description =
            "the server returned rate limit exceeded".into();
        assert!(RateLimitRetryRule.matches(&state));
    }

    #[test]
    fn no_match_normal_response() {
        let mut state = AgentState::new();
        state.short_term.last_observation = Some("HTTP 200 OK".into());
        assert!(!RateLimitRetryRule.matches(&state));
    }

    #[test]
    fn no_match_empty_state() {
        let state = AgentState::new();
        assert!(!RateLimitRetryRule.matches(&state));
    }
}
