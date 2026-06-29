//! Replay query filter.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::core::envelope::Envelope;
use crate::core::error::Result;
use crate::core::topic::TopicPattern;

/// Filter applied during replay queries. All set fields are AND-combined.
#[derive(Debug, Clone, Default)]
pub struct ReplayFilter {
    pub topic: Option<TopicPattern>,
    pub root_id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

impl ReplayFilter {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn topic(pattern: &str) -> Result<Self> {
        Ok(Self {
            topic: Some(TopicPattern::new(pattern)?),
            ..Default::default()
        })
    }

    pub fn with_root(mut self, root: Uuid) -> Self {
        self.root_id = Some(root);
        self
    }
    pub fn with_since(mut self, t: DateTime<Utc>) -> Self {
        self.since = Some(t);
        self
    }
    pub fn with_until(mut self, t: DateTime<Utc>) -> Self {
        self.until = Some(t);
        self
    }
    pub fn with_limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn matches(&self, env: &Envelope) -> bool {
        if let Some(pat) = &self.topic {
            if !pat.matches_str(&env.topic) {
                return false;
            }
        }
        if let Some(root) = self.root_id {
            if env.root_id != root {
                return false;
            }
        }
        if let Some(since) = self.since {
            if env.timestamp < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if env.timestamp > until {
                return false;
            }
        }
        true
    }

    /// Cheap topic-only check used to prune index entries before disk reads.
    pub(crate) fn topic_matches(&self, topic: &str) -> bool {
        match &self.topic {
            None => true,
            Some(pat) => pat.matches_str(topic),
        }
    }
}
