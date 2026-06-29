//! Public [`JsonlStore`] facade.
//!
//! - Spawns a background WAL writer task with **group commit**
//! - Validates record CRC on open and **truncates torn writes**
//! - Provides [`JsonlStore::shutdown`] for graceful drain + fsync

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::core::envelope::Envelope;
use crate::core::error::{EventMeshError, Result};
use crate::store::filter::ReplayFilter;
use crate::store::traits::EventStore;

use super::index::{
    RECORD_PREFIX_LEN, TopicIndex, append_idx_entries, load_and_recover,
};
use super::writer::{PendingWrite, WriterCtrl, writer_loop};

#[derive(Debug, Clone)]
pub struct JsonlStoreOptions {
    pub batch_size: usize,
    pub channel_capacity: usize,
    pub fsync: bool,
}

impl Default for JsonlStoreOptions {
    fn default() -> Self {
        Self {
            batch_size: 256,
            channel_capacity: 4096,
            fsync: false,
        }
    }
}

pub struct JsonlStore {
    log_path: PathBuf,
    idx_path: PathBuf,
    write_tx: mpsc::Sender<PendingWrite>,
    ctrl_tx: mpsc::Sender<WriterCtrl>,
    writer_handle: AsyncMutex<Option<JoinHandle<()>>>,
    index: Arc<TopicIndex>,
    count: Arc<AtomicU64>,
    read_lock: AsyncMutex<()>,
}

impl JsonlStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, JsonlStoreOptions::default()).await
    }

    pub async fn open_with(
        path: impl AsRef<Path>,
        opts: JsonlStoreOptions,
    ) -> Result<Self> {
        let log_path = path.as_ref().to_path_buf();
        let idx_path = log_path.with_extension(
            log_path
                .extension()
                .map(|e| format!("{}.idx", e.to_string_lossy()))
                .unwrap_or_else(|| "idx".into()),
        );
        if let Some(parent) = log_path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }

        let outcome = load_and_recover(&log_path, &idx_path).await?;
        let index = Arc::new(outcome.index);
        let count = Arc::new(AtomicU64::new(index.len() as u64));

        // Persist the entries we recovered from the log tail so the idx file
        // is consistent with the log on disk.
        if !outcome.pending_idx_appends.is_empty() {
            append_idx_entries(&idx_path, &outcome.pending_idx_appends).await?;
        }

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await?;
        let idx_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&idx_path)
            .await?;

        let (write_tx, write_rx) = mpsc::channel::<PendingWrite>(opts.channel_capacity);
        let (ctrl_tx, ctrl_rx) = mpsc::channel::<WriterCtrl>(64);

        let handle = tokio::spawn(writer_loop(
            log_file,
            idx_file,
            outcome.log_pos,
            write_rx,
            ctrl_rx,
            opts.batch_size,
            opts.fsync,
            index.clone(),
            count.clone(),
        ));

        Ok(Self {
            log_path,
            idx_path,
            write_tx,
            ctrl_tx,
            writer_handle: AsyncMutex::new(Some(handle)),
            index,
            count,
            read_lock: AsyncMutex::new(()),
        })
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }
    pub fn index_path(&self) -> &Path {
        &self.idx_path
    }

    /// Drain pending writes and flush (and fsync if configured).
    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.ctrl_tx
            .send(WriterCtrl::Flush(tx))
            .await
            .map_err(|_| EventMeshError::Closed)?;
        rx.await.map_err(|_| EventMeshError::Closed)?
    }

    /// Drain everything pending, fsync, then **stop the writer task**.
    /// Subsequent appends fail with `EventMeshError::Closed`.
    pub async fn shutdown(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        // Best-effort send; if writer is already gone, treat as success.
        if self.ctrl_tx.send(WriterCtrl::Shutdown(tx)).await.is_err() {
            return Ok(());
        }
        let r = rx.await.map_err(|_| EventMeshError::Closed)?;
        if let Some(h) = self.writer_handle.lock().await.take() {
            let _ = h.await;
        }
        r
    }

    async fn submit(&self, env: Arc<Envelope>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.write_tx
            .send(PendingWrite { env, ack: tx })
            .await
            .map_err(|_| EventMeshError::Closed)?;
        rx.await.map_err(|_| EventMeshError::Closed)?
    }
}

#[async_trait]
impl EventStore for JsonlStore {
    async fn append(&self, env: Arc<Envelope>) -> Result<()> {
        self.submit(env).await
    }

    async fn append_batch(&self, envs: Vec<Arc<Envelope>>) -> Result<()> {
        let mut acks = Vec::with_capacity(envs.len());
        for env in envs {
            let (tx, rx) = oneshot::channel();
            self.write_tx
                .send(PendingWrite { env, ack: tx })
                .await
                .map_err(|_| EventMeshError::Closed)?;
            acks.push(rx);
        }
        for rx in acks {
            rx.await.map_err(|_| EventMeshError::Closed)??;
        }
        Ok(())
    }

    async fn query(&self, filter: &ReplayFilter) -> Result<Vec<Arc<Envelope>>> {
        // Make sure pending writes are visible.
        self.flush().await?;

        let since_ms = filter.since.map(|t| t.timestamp_millis());
        let until_ms = filter.until.map(|t| t.timestamp_millis());
        let mut candidates = self.index.snapshot_filtered_time(
            |t| filter.topic_matches(t),
            since_ms,
            until_ms,
        );
        if filter.topic.is_some() && filter.root_id.is_none() {
            if let Some(lim) = filter.limit {
                candidates.truncate(lim);
            }
        }

        let _g = self.read_lock.lock().await;
        let mut f = File::open(&self.log_path).await?;
        let mut out = Vec::new();
        let mut buf = Vec::with_capacity(4096);
        for c in candidates {
            buf.resize(c.len as usize, 0u8);
            f.seek(std::io::SeekFrom::Start(c.offset)).await?;
            f.read_exact(&mut buf).await?;
            // Strip CRC prefix + trailing newline.
            if buf.len() <= RECORD_PREFIX_LEN {
                continue;
            }
            let json_end = if buf.last() == Some(&b'\n') {
                buf.len() - 1
            } else {
                buf.len()
            };
            let json_slice = &buf[RECORD_PREFIX_LEN..json_end];
            let env: Envelope = match serde_json::from_slice(json_slice) {
                Ok(e) => e,
                Err(_) => continue,
            };
            if filter.matches(&env) {
                out.push(Arc::new(env));
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
        Ok(self.count.load(Ordering::Acquire) as usize)
    }

    async fn flush(&self) -> Result<()> {
        JsonlStore::flush(self).await
    }

    async fn shutdown(&self) -> Result<()> {
        JsonlStore::shutdown(self).await
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
            "uwu_event_mesh_test_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn roundtrip_with_index() {
        let dir = tempdir();
        let path = dir.join("events.jsonl");
        let s = JsonlStore::open(&path).await.unwrap();
        let t = Topic::new("x.y").unwrap();
        for i in 0..5u32 {
            s.append(Arc::new(Envelope::new(&t, json!({ "i": i }))))
                .await
                .unwrap();
        }
        s.flush().await.unwrap();
        assert_eq!(s.len().await.unwrap(), 5);
        assert!(s.index_path().exists());
        let r = s.query(&ReplayFilter::all()).await.unwrap();
        assert_eq!(r.len(), 5);
        assert_eq!(r[4].payload["i"], 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn batch_topic_index_pruning() {
        let dir = tempdir();
        let path = dir.join("events.jsonl");
        let s = JsonlStore::open(&path).await.unwrap();
        let ta = Topic::new("a.x").unwrap();
        let tb = Topic::new("b.x").unwrap();
        let mut batch = Vec::new();
        for i in 0..50u32 {
            let t = if i % 2 == 0 { &ta } else { &tb };
            batch.push(Arc::new(Envelope::new(t, json!({ "i": i }))));
        }
        s.append_batch(batch).await.unwrap();
        s.flush().await.unwrap();
        let r = s.query(&ReplayFilter::topic("a.>").unwrap()).await.unwrap();
        assert_eq!(r.len(), 25);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn reopen_loads_index() {
        let dir = tempdir();
        let path = dir.join("events.jsonl");
        {
            let s = JsonlStore::open(&path).await.unwrap();
            let t = Topic::new("z.z").unwrap();
            s.append(Arc::new(Envelope::new(&t, json!({"n": 1}))))
                .await
                .unwrap();
            s.shutdown().await.unwrap();
        }
        let s2 = JsonlStore::open(&path).await.unwrap();
        assert_eq!(s2.len().await.unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn corrupt_tail_is_truncated() {
        let dir = tempdir();
        let path = dir.join("events.jsonl");
        // Write a few good records.
        {
            let s = JsonlStore::open(&path).await.unwrap();
            let t = Topic::new("ok.ok").unwrap();
            for i in 0..3u32 {
                s.append(Arc::new(Envelope::new(&t, json!({ "i": i }))))
                    .await
                    .unwrap();
            }
            s.shutdown().await.unwrap();
        }
        // Append a torn line at the end (bad CRC + missing newline).
        {
            use tokio::io::AsyncWriteExt;
            let mut f = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await
                .unwrap();
            f.write_all(b"deadbeef\t{partial").await.unwrap();
            f.flush().await.unwrap();
        }
        // Reopen — torn tail must be truncated, prior records preserved.
        let s = JsonlStore::open(&path).await.unwrap();
        assert_eq!(s.len().await.unwrap(), 3);
        let r = s.query(&ReplayFilter::all()).await.unwrap();
        assert_eq!(r.len(), 3);
        // Append should still work after recovery.
        let t = Topic::new("ok.ok").unwrap();
        s.append(Arc::new(Envelope::new(&t, json!({"i": 99}))))
            .await
            .unwrap();
        s.shutdown().await.unwrap();
        let s2 = JsonlStore::open(&path).await.unwrap();
        assert_eq!(s2.len().await.unwrap(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn shutdown_then_use_fails() {
        let dir = tempdir();
        let path = dir.join("events.jsonl");
        let s = JsonlStore::open(&path).await.unwrap();
        let t = Topic::new("g.k").unwrap();
        s.append(Arc::new(Envelope::new(&t, json!({"n": 1}))))
            .await
            .unwrap();
        s.shutdown().await.unwrap();
        let r = s
            .append(Arc::new(Envelope::new(&t, json!({"n": 2}))))
            .await;
        assert!(r.is_err(), "append after shutdown must fail");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn time_range_index_prunes() {
        use chrono::Duration;
        let dir = tempdir();
        let path = dir.join("events.jsonl");
        let s = JsonlStore::open(&path).await.unwrap();
        let t = Topic::new("tr.k").unwrap();
        // Write 5 events with synthetic timestamps stretching across a window.
        let base = chrono::Utc::now();
        for i in 0..5i64 {
            let mut env = Envelope::new(&t, json!({ "i": i }));
            env.timestamp = base + Duration::milliseconds(i * 1000);
            s.append(Arc::new(env)).await.unwrap();
        }
        s.flush().await.unwrap();
        // Slice the middle: [base+1s, base+3s] inclusive.
        let f = ReplayFilter::topic("tr.>")
            .unwrap()
            .with_since(base + Duration::milliseconds(1000))
            .with_until(base + Duration::milliseconds(3000));
        let r = s.query(&f).await.unwrap();
        let got: Vec<i64> = r
            .iter()
            .map(|e| e.payload["i"].as_i64().unwrap())
            .collect();
        assert_eq!(got, vec![1, 2, 3]);
        s.shutdown().await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
