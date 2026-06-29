//! Metadata attached to every [`SerializedEnvelope`] and optionally to
//! [`Envelope`] for observability, TTL enforcement, and producer attribution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Standard metadata carried by every envelope in the mesh ecosystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Wall-clock time when the event was first produced.
    pub produced_at: DateTime<Utc>,
    /// Logical producer (agent id, service name, etc.).
    pub producer_id: String,
    /// Optional time-to-live. The broker MAY drop expired envelopes
    /// before delivery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<Duration>,
}

impl EventMetadata {
    pub fn new(producer_id: impl Into<String>) -> Self {
        Self {
            produced_at: Utc::now(),
            producer_id: producer_id.into(),
            ttl: None,
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// True if `ttl` is set and has elapsed since `produced_at`.
    pub fn is_expired(&self) -> bool {
        let Some(ttl) = self.ttl else {
            return false;
        };
        let elapsed = Utc::now()
            .signed_duration_since(self.produced_at)
            .num_milliseconds()
            .max(0) as u64;
        elapsed > ttl.as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn expiry() {
        let meta = EventMetadata::new("test").with_ttl(Duration::from_millis(100));
        assert!(!meta.is_expired());
        sleep(Duration::from_millis(150));
        assert!(meta.is_expired());
    }

    #[test]
    fn no_ttl_never_expires() {
        let meta = EventMetadata::new("test");
        assert!(!meta.is_expired());
    }
}
