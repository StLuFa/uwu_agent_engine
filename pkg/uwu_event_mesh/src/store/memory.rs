//! In-memory ring-buffer store.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use crate::core::envelope::Envelope;
use crate::core::error::Result;

use crate::store::ReplayFilter;
use super::traits::EventStore;

pub struct MemoryStore {
    cap: usize,
    buf: Mutex<VecDeque<Arc<Envelope>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::with_capacity(usize::MAX)
    }
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            cap,
            buf: Mutex::new(VecDeque::new()),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventStore for MemoryStore {
    async fn append(&self, env: Arc<Envelope>) -> Result<()> {
        let mut buf = self.buf.lock();
        if buf.len() == self.cap {
            buf.pop_front();
        }
        buf.push_back(env);
        Ok(())
    }

    async fn append_batch(&self, envs: Vec<Arc<Envelope>>) -> Result<()> {
        let mut buf = self.buf.lock();
        for e in envs {
            if buf.len() == self.cap {
                buf.pop_front();
            }
            buf.push_back(e);
        }
        Ok(())
    }

    async fn query(&self, filter: &ReplayFilter) -> Result<Vec<Arc<Envelope>>> {
        let buf = self.buf.lock();
        let mut out = Vec::new();
        for e in buf.iter() {
            if filter.matches(e) {
                out.push(e.clone());
                if let Some(lim) = filter.limit {
                    if out.len() >= lim {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    async fn len(&self) -> Result<usize> {
        Ok(self.buf.lock().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::topic::Topic;
    use serde_json::json;

    #[tokio::test]
    async fn filter_topic() {
        let s = MemoryStore::new();
        let t1 = Topic::new("a.b").unwrap();
        let t2 = Topic::new("a.c").unwrap();
        s.append(Arc::new(Envelope::new(&t1, json!({"n": 1}))))
            .await
            .unwrap();
        s.append(Arc::new(Envelope::new(&t2, json!({"n": 2}))))
            .await
            .unwrap();

        let r = s.query(&ReplayFilter::topic("a.b").unwrap()).await.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload["n"], 1);

        let r = s.query(&ReplayFilter::topic("a.>").unwrap()).await.unwrap();
        assert_eq!(r.len(), 2);
    }
}
