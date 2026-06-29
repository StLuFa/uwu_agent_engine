//! Append-only JSON-Lines store with WAL group commit and a per-topic
//! secondary index. See [`JsonlStore`].

mod index;
mod store;
mod writer;

pub use store::{JsonlStore, JsonlStoreOptions};
