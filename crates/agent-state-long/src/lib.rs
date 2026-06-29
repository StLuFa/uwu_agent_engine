//! # agent-state-long
//!
//! 长程工作状态（独立 crate，可选）—— LongTermWS 的精细定义。
//!
//! LongTermWS 承载任务级数据：进度、累积 JEPA 预测误差、预算消耗。

use agent_state::LongTermWS;

// Re-export from agent-state
pub use LongTermWS;
