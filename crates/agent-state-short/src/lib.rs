//! # agent-state-short
//!
//! 短程工作状态（独立 crate，可选）—— ShortTermWS 的精细定义。
//!
//! 将 ShortTermWS 拆分为独立 crate 便于：
//! - 热路径优化（短程状态每步更新，独立编译单元允许更激进的内联）
//! - 可选依赖（Sidecar 只需快照时可只依赖此 crate）

use agent_state::ShortTermWS;

// Re-export from agent-state
pub use ShortTermWS;
