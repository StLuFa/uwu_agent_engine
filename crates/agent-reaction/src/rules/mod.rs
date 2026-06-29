//! Built-in reaction rules

mod captcha;
mod idle;
mod popup_close;
mod rate_limit;

pub use captcha::CaptchaDetectRule;
pub use idle::IdleTimeoutRule;
pub use popup_close::PopupCloseRule;
pub use rate_limit::RateLimitRetryRule;
