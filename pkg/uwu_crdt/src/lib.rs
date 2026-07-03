//! # uwu-crdt
//!
//! uwu 引擎的 CRDT（Conflict-free Replicated Data Types）**合并计算层** —— 纯内存、无 I/O。
//!
//! ## 两类能力
//!
//! 1. [`primitives`]：手写状态型 CRDT 原语（`GCounter` / `PNCounter` / `LWWRegister` /
//!    `ORSet` / `VectorClock`），用于多 Agent 共享状态合并。
//! 2. [`doc`]：基于 [Loro](https://loro.dev) 的 [`UwuCrdtDoc`] 封装 —— 面向文档/图的
//!    可移动树 CRDT。供 `uwu_wiki::wiki-collab` 的 Block 树协作、`agent-context-db` 的
//!    可合并子域复用。
//!
//! ## 设计定位
//!
//! `uwu-crdt` **只做合并计算，不持有存储**：合并后的状态与 Op 序列由调用方
//! （`WikiStorage` / context-db）持久化到 DB。DB 是唯一真相源，本 crate 是合并算子。
//!
//! `UwuCrdtDoc` 暴露**领域无关**的树操作 + [`UwuOp`] 枚举；wiki / context-db 各自把
//! 自己的领域操作翻译为 `UwuOp`，因此本 crate **不反向依赖任何领域 crate**。

pub mod doc;
pub mod primitives;

pub use doc::{NodeId, UwuCrdtDoc, UwuCrdtError, UwuOp};
pub use primitives::{CRDTMerge, GCounter, LWWRegister, LwwMap, ORSet, PNCounter, VectorClock, merge};
