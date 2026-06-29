//! Event persistence and replay.
//!
//! ## Implementations
//!
//! - [`MemoryStore`] — bounded in-memory ring; fast, lossy past `cap`.
//! - [`JsonlStore`] — durable JSON-Lines on disk with **WAL group commit**
//!   and a **per-topic secondary index** so replay seeks directly without
//!   scanning the entire log.
//!
//! Plug your own backend (Postgres, Kafka, S3, …) by implementing
//! [`EventStore`].

mod filter;
mod jsonl;
mod memory;
mod segmented;
mod traits;

pub use filter::ReplayFilter;
pub use jsonl::{JsonlStore, JsonlStoreOptions};
pub use memory::MemoryStore;
pub use segmented::{SegmentedStore, SegmentedStoreOptions};
pub use traits::EventStore;
