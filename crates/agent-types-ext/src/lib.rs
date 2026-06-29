//! # agent-types-ext
//!
//! 业务类型（可迭代）—— 业务层的扩展类型定义。
//!
//! 与 `agent-types-core` 不同，此 crate 的类型可能随业务需求变化。
//!
//! ## 主要类型
//!
//! - `TaskManifest` — 任务清单（参与者 + 委派策略 + 结算）
//! - `AgentCard` — Agent 能力名片
//! - `SettlementPolicy` — 结算策略
//! - `SubtaskDag` — 子任务 DAG
//! - `DelegationPolicy` — 委派策略

mod agent_card;
mod delegation_policy;
mod settlement_policy;
mod task_manifest;

pub use agent_card::{AgentCard, AgentEndpoint, TaskRole};
pub use delegation_policy::{DelegationPolicy, DiscoveryStrategy, FallbackStrategy};
pub use settlement_policy::{SettlementMode, SettlementPolicy};
pub use task_manifest::TaskManifest;
