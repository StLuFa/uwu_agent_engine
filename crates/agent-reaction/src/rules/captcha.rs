//! CaptchaDetectRule —— 检测验证码，请求人工介入

use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

use super::super::ReactionRule;

/// 验证码关键词列表
const CAPTCHA_KEYWORDS: &[&str] = &[
    "captcha",
    "recaptcha",
    "hcaptcha",
    "verify you are human",
    "verify that you are human",
    "not a robot",
    "i am not a robot",
    "验证码",
    "人机验证",
    "图片验证",
    "滑块验证",
    "security check",
    "are you a human",
];

/// 检测验证码 UI → 请求人工介入
pub struct CaptchaDetectRule;

#[async_trait]
impl ReactionRule for CaptchaDetectRule {
    fn matches(&self, state: &AgentState) -> bool {
        let text = &state.short_term.current_context.description;
        if text.is_empty() {
            return false;
        }
        let lower = text.to_lowercase();
        CAPTCHA_KEYWORDS
            .iter()
            .any(|kw| lower.contains(&kw.to_lowercase()))
    }

    async fn react(&self, _state: &AgentState) -> Action {
        Action::new(
            "request_human",
            ActionParams::new().with("reason", "captcha"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_state::AgentState;

    #[test]
    fn matches_captcha_text() {
        let mut state = AgentState::new();
        state.short_term.current_context.description =
            "please complete the captcha to continue".into();
        assert!(CaptchaDetectRule.matches(&state));
    }

    #[test]
    fn matches_recaptcha() {
        let mut state = AgentState::new();
        state.short_term.current_context.description =
            "this site is protected by reCAPTCHA".into();
        assert!(CaptchaDetectRule.matches(&state));
    }

    #[test]
    fn matches_chinese_captcha() {
        let mut state = AgentState::new();
        state.short_term.current_context.description = "请完成验证码".into();
        assert!(CaptchaDetectRule.matches(&state));
    }

    #[test]
    fn no_match_normal_text() {
        let mut state = AgentState::new();
        state.short_term.current_context.description = "please log in with your username".into();
        assert!(!CaptchaDetectRule.matches(&state));
    }

    #[test]
    fn no_match_empty() {
        let state = AgentState::new();
        assert!(!CaptchaDetectRule.matches(&state));
    }
}
