//! NATS connection configuration.

use std::time::Duration;

/// NATS connection and JetStream configuration.
#[derive(Debug, Clone)]
pub struct NatsConfig {
    /// NATS server URL (e.g. `"nats://localhost:4222"`).
    pub url: String,

    /// Connection name (appears in NATS monitoring).
    pub connection_name: String,

    /// JetStream stream max age (events older than this are auto-deleted).
    /// Default: 7 days for consolidation, 1 hour for monitoring.
    pub jetstream_max_age: Duration,

    /// JetStream stream max bytes per channel.
    /// Default: 256 MiB.
    pub jetstream_max_bytes: i64,

    /// Reconnect attempts. Default: unlimited.
    pub max_reconnects: Option<usize>,

    /// Reconnect delay between attempts. Default: 500ms.
    pub reconnect_delay: Duration,
}

impl NatsConfig {
    /// Configuration for the main agent process.
    ///
    /// Connection name = agent type + session id for observability.
    pub fn for_agent(url: impl Into<String>, agent_name: &str, session_id: &str) -> Self {
        Self {
            url: url.into(),
            connection_name: format!("agent-{agent_name}-{session_id}"),
            jetstream_max_age: Duration::from_secs(7 * 86400), // 7 days
            jetstream_max_bytes: 256 * 1024 * 1024,            // 256 MiB
            max_reconnects: None,                               // unlimited
            reconnect_delay: Duration::from_millis(500),
        }
    }

    /// Configuration for a sidecar process.
    pub fn for_sidecar(url: impl Into<String>, sidecar_name: &str) -> Self {
        Self {
            url: url.into(),
            connection_name: format!("sidecar-{sidecar_name}"),
            jetstream_max_age: Duration::from_secs(86400),  // 1 day
            jetstream_max_bytes: 128 * 1024 * 1024,         // 128 MiB
            max_reconnects: None,
            reconnect_delay: Duration::from_millis(500),
        }
    }
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: "nats://localhost:4222".into(),
            connection_name: "uwu-agent".into(),
            jetstream_max_age: Duration::from_secs(7 * 86400),
            jetstream_max_bytes: 256 * 1024 * 1024,
            max_reconnects: None,
            reconnect_delay: Duration::from_millis(500),
        }
    }
}
