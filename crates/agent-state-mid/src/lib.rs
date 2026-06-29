//! # agent-state-mid
//!
//! 中程工作状态（独立 crate，可选）—— MidTermWS 的精细定义。
//!
//! MidTermWS 承载动作历史、已知事实和交互模式检测，
//! 是 Metacognition 消费的关键输入（recent_pattern → SwitchStrategy）。

pub use agent_state::MidTermWS;
