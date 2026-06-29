//! `EventStore` trait — append-only log abstraction.

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::envelope::Envelope;
use crate::core::error::Result;

use crate::store::ReplayFilter;

#[async_trait]
pub trait EventStore: Send + Sync + 'static {
    async fn append(&self, env: Arc<Envelope>) -> Result<()>;

    /// Append a batch. Default impl appends sequentially. Implementations
    /// SHOULD coalesce into a single fsync where possible.
    async fn append_batch(&self, envs: Vec<Arc<Envelope>>) -> Result<()> {
        for e in envs {
            self.append(e).await?;
        }
        Ok(())
    }

    /// Return all envelopes matching `filter`, in publish order.
    async fn query(&self, filter: &ReplayFilter) -> Result<Vec<Arc<Envelope>>>;

    /// Total persisted count (best-effort).
    async fn len(&self) -> Result<usize>;

    /// Force buffered writes to durable storage. Default: no-op.
    async fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// Stop background workers, drain pending writes, fsync. After
    /// `shutdown`, further `append` calls SHOULD fail.
    async fn shutdown(&self) -> Result<()> {
        self.flush().await
    }
}
