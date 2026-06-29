//! Cross-process / cross-mesh bridge.
//!
//! A [`Bridge`] is anything that can ferry envelopes between an
//! [`crate::EventMesh`] instance and the outside world (another mesh, a
//! NATS/Redis cluster, a websocket peer, etc.). The mesh calls
//! [`Bridge::publish_remote`] for every locally-published envelope. The
//! bridge implementation is responsible for delivering those envelopes to
//! the remote side and for feeding inbound envelopes back into the local
//! mesh via [`crate::EventMesh::ingest_remote`].
//!
//! A simple in-process [`ChannelBridge`] is provided for tests, examples
//! and same-runtime mesh federation.

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::envelope::Envelope;
use crate::core::error::Result;

/// Bridge interface. Implementations forward locally-published envelopes
/// out to a remote transport.
#[async_trait]
pub trait Bridge: Send + Sync + 'static {
    /// Forward a locally-published envelope to the remote side.
    ///
    /// Implementations must not call back into the publishing mesh from
    /// inside this method (use a background task to ingest remote events).
    async fn publish_remote(&self, env: Arc<Envelope>) -> Result<()>;
}

mod channel;
pub use channel::{ChannelBridge, ChannelBridgePair};
