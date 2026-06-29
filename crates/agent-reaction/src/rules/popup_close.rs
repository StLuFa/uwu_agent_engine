//! PopupCloseRule —— 检测弹窗关闭按钮，自动点击

use agent_state::AgentState;
use agent_types_core::{Action, ActionParams};
use async_trait::async_trait;

use super::super::ReactionRule;

/// 弹窗关键词列表（大小写不敏感）
const POPUP_KEYWORDS: &[&str] = &[
    "popup",
    "modal",
    "dialog",
    "overlay",
    "close",
    "dismiss",
    "弹窗",
    "广告",
    "关闭",
    "×",
    "✕",
    "x",
    "accept cookies",
    "gdpr",
    "newsletter",
    "subscribe",
];

/// 检测弹窗 UI 元素 → 返回关闭点击动作
pub struct PopupCloseRule;

#[async_trait]
impl ReactionRule for PopupCloseRule {
    fn matches(&self, state: &AgentState) -> bool {
        let text = &state.short_term.current_context.description;
        if text.is_empty() {
            return false;
        }
        let lower = text.to_lowercase();
        POPUP_KEYWORDS
            .iter()
            .any(|kw| lower.contains(&kw.to_lowercase()))
    }

    async fn react(&self, _state: &AgentState) -> Action {
        Action::new(
            "click",
            ActionParams::new().with("target", "popup-close-button"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_state::AgentState;

    #[test]
    fn matches_popup_text() {
        let mut state = AgentState::new();
        state.short_term.current_context.description = "a newsletter popup appeared".into();
        assert!(PopupCloseRule.matches(&state));
    }

    #[test]
    fn matches_chinese_popup() {
        let mut state = AgentState::new();
        state.short_term.current_context.description = "弹窗广告出现了".into();
        assert!(PopupCloseRule.matches(&state));
    }

    #[test]
    fn no_match_normal_text() {
        let mut state = AgentState::new();
        state.short_term.current_context.description = "the page loaded successfully".into();
        assert!(!PopupCloseRule.matches(&state));
    }

    #[test]
    fn no_match_empty_text() {
        let state = AgentState::new();
        assert!(!PopupCloseRule.matches(&state));
    }
}
