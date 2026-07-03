//! # agent-context-db-uwu (M3 专有扩展)
//!
//! 通用核心（L1-L7）之上叠加 uwu 五维深度耦合：
//!
//! - [`state_bridge`]：State 快照读写 + fork 推演沙盒
//! - [`metacog_bridge`]：校准数据冷归档 + 检索（事实层/派生层分离，见 §6.3）
//! - [`character_constraint`]：核心价值观作为 write 前置约束
//!
//! ## 解耦约束
//!
//! - 强依赖 core / retrieve / version，但**通过注入的窄端口**访问存储，
//!   不 `use` 任何后端具体 struct。
//! - 真实五维类型（`AgentState` / `CalibrationRecord` 等）在对接时替换本模块占位类型；
//!   本 crate 只定义与之对齐的接口形状。

pub mod character_constraint;
pub mod metacog_bridge;
pub mod state_bridge;

pub use character_constraint::{CharacterConstraint, ConstraintViolation, CoreValue};
pub use metacog_bridge::{MetacogBridge, PredErrorSample, TimeWindow};
pub use state_bridge::{ForkHandle, StateBridge, StateSnapshot};
