//! # agent-types-core
//!
//! 基础类型（冻结）—— 全系统共享的核心类型定义。
//!
//! 此 crate 零外部依赖（除 serde/chrono/uuid），
//! 所有其他 Agent crate 均可安全依赖它而不引入循环。
//!
//! ## 主要类型
//!
//! - `AgentId` — Agent 全局唯一标识
//! - `Action` / `ActionParams` — Agent 可执行动作
//! - `Layer<I, O>` — 通用管道层 trait
//! - `Uncertain<T>` — 带置信度的值

mod action;
mod agent_id;
mod layer;
mod uncertain;

pub use action::{Action, ActionParams, ActionStatus};
pub use agent_id::AgentId;
pub use layer::Layer;
pub use uncertain::Uncertain;
