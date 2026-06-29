//! In-process channel-based bridge for federating two meshes inside the
//! same Tokio runtime, useful for tests, examples, or sharding work
//! across mesh instances without a network hop.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::core::envelope::Envelope;
use crate::core::error::Result;

use super::Bridge;

/// Bridge endpoint backed by an unbounded mpsc channel.
pub struct ChannelBridge {
    tx: mpsc::UnboundedSender<Arc<Envelope>>,
}

#[async_trait]
impl Bridge for ChannelBridge {
    async fn publish_remote(&self, env: Arc<Envelope>) -> Result<()> {
        // Drop on closed peer — bridge errors must not surface to publishers.
        let _ = self.tx.send(env);
        Ok(())
    }
}

/// A pair of bridges plus their inbound receivers, ready to wire into two
/// meshes. Attach `a_to_b` to mesh A and feed its events into mesh B's
/// `ingest_remote` (driven by `b_inbox`); symmetrically for the other side.
pub struct ChannelBridgePair {
    pub a_to_b: Arc<ChannelBridge>,
    pub b_to_a: Arc<ChannelBridge>,
    pub a_inbox: mpsc::UnboundedReceiver<Arc<Envelope>>,
    pub b_inbox: mpsc::UnboundedReceiver<Arc<Envelope>>,
}

impl ChannelBridgePair {
    pub fn new() -> Self {
        let (tx_ab, rx_ab) = mpsc::unbounded_channel();
        let (tx_ba, rx_ba) = mpsc::unbounded_channel();
        Self {
            a_to_b: Arc::new(ChannelBridge { tx: tx_ab }),
            b_to_a: Arc::new(ChannelBridge { tx: tx_ba }),
            // Each mesh consumes from the channel that the OTHER bridge writes.
            a_inbox: rx_ba,
            b_inbox: rx_ab,
        }
    }
}

impl Default for ChannelBridgePair {
    fn default() -> Self {
        Self::new()
    }
}
