//! Directory-based segmented store: a sequence of [`JsonlStore`] segments
//! with size-based rotation.
//!
//! Layout under `dir/`:
//!
//! ```text
//! 00000001.jsonl
//! 00000001.jsonl.idx
//! 00000002.jsonl
//! 00000002.jsonl.idx
//! ...
//! ```
//!
//! Active segment is the one with the highest id. When it grows past
//! `max_segment_bytes` after a successful append, the segment is sealed
//! (graceful shutdown + fsync) and a fresh segment with id+1 is opened
//! for subsequent writes.
//!
//! `query()` walks every segment in id order; per-segment time/topic
//! pruning (via the secondary index) keeps cold segments cheap.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use tokio::sync::Mutex as AsyncMutex;

use crate::core::envelope::Envelope;
use crate::core::error::Result;
use crate::store::filter::ReplayFilter;
use crate::store::jsonl::{JsonlStore, JsonlStoreOptions};
use crate::store::traits::EventStore;

#[derive(Debug, Clone)]
pub struct SegmentedStoreOptions {
    pub max_segment_bytes: u64,
    pub jsonl: JsonlStoreOptions,
}

impl Default for SegmentedStoreOptions {
    fn default() -> Self {
        Self {
            max_segment_bytes: 64 * 1024 * 1024,
            jsonl: JsonlStoreOptions::default(),
        }
    }
}

struct SegmentEntry {
    id: u64,
    store: Arc<JsonlStore>,
}

pub struct SegmentedStore {
    dir: PathBuf,
    opts: SegmentedStoreOptions,
    segments: RwLock<Vec<SegmentEntry>>,
    /// Serializes append to make rotation decisions consistent.
    append_lock: AsyncMutex<()>,
}

impl SegmentedStore {
    pub async fn open(dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(dir, SegmentedStoreOptions::default()).await
    }

    pub async fn open_with(
        dir: impl AsRef<Path>,
        opts: SegmentedStoreOptions,
    ) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&dir).await?;

        // Discover existing segments by scanning for files named NNNNNNNN.jsonl.
        let mut ids: Vec<u64> = Vec::new();
        let mut rd = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = rd.next_entry().await? {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = match p.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            if let Ok(id) = stem.parse::<u64>() {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        if ids.is_empty() {
            ids.push(1);
        }

        let mut segments = Vec::with_capacity(ids.len());
        for id in ids {
            let path = segment_path(&dir, id);
            let store = JsonlStore::open_with(&path, opts.jsonl.clone()).await?;
            segments.push(SegmentEntry {
                id,
                store: Arc::new(store),
            });
        }

        Ok(Self {
            dir,
            opts,
            segments: RwLock::new(segments),
            append_lock: AsyncMutex::new(()),
        })
    }

    fn current(&self) -> Arc<JsonlStore> {
        self.segments.read().last().unwrap().store.clone()
    }

    fn current_id(&self) -> u64 {
        self.segments.read().last().unwrap().id
    }

    fn snapshot(&self) -> Vec<Arc<JsonlStore>> {
        self.segments
            .read()
            .iter()
            .map(|s| s.store.clone())
            .collect()
    }

    /// Force an immediate rotation: seal current segment, open a new one.
    pub async fn rotate(&self) -> Result<()> {
        let _g = self.append_lock.lock().await;
        let next_id = self.current_id() + 1;
        // Flush current; we keep it open for reads (query walks all).
        let cur = self.current();
        cur.flush().await?;
        let path = segment_path(&self.dir, next_id);
        let store = JsonlStore::open_with(&path, self.opts.jsonl.clone()).await?;
        self.segments.write().push(SegmentEntry {
            id: next_id,
            store: Arc::new(store),
        });
        Ok(())
    }

    async fn maybe_rotate(&self) -> Result<()> {
        // Inspect current segment size by stat; cheaper than tracking bytes.
        let path = segment_path(&self.dir, self.current_id());
        let size = match tokio::fs::metadata(&path).await {
            Ok(m) => m.len(),
            Err(_) => return Ok(()),
        };
        if size >= self.opts.max_segment_bytes {
            let next_id = self.current_id() + 1;
            self.current().flush().await?;
            let new_path = segment_path(&self.dir, next_id);
            let store = JsonlStore::open_with(&new_path, self.opts.jsonl.clone()).await?;
            self.segments.write().push(SegmentEntry {
                id: next_id,
                store: Arc::new(store),
            });
        }
        Ok(())
    }
}

fn segment_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id:08}.jsonl"))
}

#[async_trait]
impl EventStore for SegmentedStore {
    async fn append(&self, env: Arc<Envelope>) -> Result<()> {
        let _g = self.append_lock.lock().await;
        let cur = self.current();
        cur.append(env).await?;
        self.maybe_rotate().await?;
        Ok(())
    }

    async fn append_batch(&self, envs: Vec<Arc<Envelope>>) -> Result<()> {
        let _g = self.append_lock.lock().await;
        let cur = self.current();
        cur.append_batch(envs).await?;
        self.maybe_rotate().await?;
        Ok(())
    }

    async fn query(&self, filter: &ReplayFilter) -> Result<Vec<Arc<Envelope>>> {
        let segments = self.snapshot();
        let mut out = Vec::new();
        let limit = filter.limit;
        for seg in segments {
            // Per-segment query already honours topic + time pruning.
            let mut sub_filter = filter.clone();
            // Within a segment we still want full limit handling at this level.
            sub_filter.limit = limit;
            let part = seg.query(&sub_filter).await?;
            out.extend(part);
            if let Some(lim) = limit {
                if out.len() >= lim {
                    out.truncate(lim);
                    break;
                }
            }
        }
        Ok(out)
    }

    async fn len(&self) -> Result<usize> {
        let segments = self.snapshot();
        let mut total = 0;
        for seg in segments {
            total += seg.len().await?;
        }
        Ok(total)
    }

    async fn flush(&self) -> Result<()> {
        for seg in self.snapshot() {
            seg.flush().await?;
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for seg in self.snapshot() {
            seg.shutdown().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::topic::Topic;
    use serde_json::json;

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "uwu_segmented_test_{}",
            uuid::Uuid::new_v4().simple()
        ));
        p
    }

    #[tokio::test]
    async fn rotates_when_segment_full() {
        let dir = tempdir();
        let mut opts = SegmentedStoreOptions::default();
        opts.max_segment_bytes = 512; // tiny — force rotation quickly
        let s = SegmentedStore::open_with(&dir, opts).await.unwrap();
        let t = Topic::new("seg.k").unwrap();
        for i in 0..50u32 {
            s.append(Arc::new(Envelope::new(&t, json!({ "i": i }))))
                .await
                .unwrap();
        }
        s.flush().await.unwrap();
        // Multiple segments must exist.
        let mut count = 0;
        let mut rd = tokio::fs::read_dir(&dir).await.unwrap();
        while let Some(e) = rd.next_entry().await.unwrap() {
            if e.path().extension().and_then(|x| x.to_str()) == Some("jsonl") {
                count += 1;
            }
        }
        assert!(count >= 2, "expected rotation, got {count} segments");
        // All 50 events readable through unified query.
        let r = s.query(&ReplayFilter::all()).await.unwrap();
        assert_eq!(r.len(), 50);
        s.shutdown().await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reopens_existing_segments() {
        let dir = tempdir();
        let t = Topic::new("ro.k").unwrap();
        {
            let mut opts = SegmentedStoreOptions::default();
            opts.max_segment_bytes = 256;
            let s = SegmentedStore::open_with(&dir, opts).await.unwrap();
            for i in 0..20u32 {
                s.append(Arc::new(Envelope::new(&t, json!({ "i": i }))))
                    .await
                    .unwrap();
            }
            s.shutdown().await.unwrap();
        }
        let s2 = SegmentedStore::open(&dir).await.unwrap();
        let r = s2.query(&ReplayFilter::all()).await.unwrap();
        assert_eq!(r.len(), 20);
        s2.shutdown().await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
