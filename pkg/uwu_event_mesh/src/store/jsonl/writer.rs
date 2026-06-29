//! Background WAL writer task.
//!
//! - Owns log + idx file handles
//! - Drains pending writes from a channel, batching multiple envelopes into a
//!   single `write_all + flush`
//! - Records use `<crc32_hex>\t<json>\n` for crash detection on next open
//! - Listens for an explicit `Shutdown` signal so callers can wait for all
//!   in-flight writes to land before dropping the store

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};

use crate::core::envelope::Envelope;
use crate::core::error::{EventMeshError, Result};

use super::index::{IndexEntry, TopicIndex, encode_record};

pub(crate) struct PendingWrite {
    pub env: Arc<Envelope>,
    pub ack: oneshot::Sender<Result<()>>,
}

/// Control signals sent to the writer task.
pub(crate) enum WriterCtrl {
    /// Drain any pending writes, flush, then ack.
    Flush(oneshot::Sender<Result<()>>),
    /// Same as Flush but the writer terminates after acking.
    Shutdown(oneshot::Sender<Result<()>>),
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn writer_loop(
    mut log_file: File,
    mut idx_file: File,
    mut log_pos: u64,
    mut write_rx: mpsc::Receiver<PendingWrite>,
    mut ctrl_rx: mpsc::Receiver<WriterCtrl>,
    batch_size: usize,
    fsync: bool,
    index: Arc<TopicIndex>,
    count: Arc<AtomicU64>,
) {
    let mut batch: Vec<PendingWrite> = Vec::with_capacity(batch_size);
    let mut log_buf = Vec::with_capacity(64 * 1024);
    let mut idx_buf = Vec::with_capacity(8 * 1024);
    let mut new_entries: Vec<IndexEntry> = Vec::with_capacity(batch_size);

    loop {
        tokio::select! {
            biased;
            Some(ctrl) = ctrl_rx.recv() => {
                match ctrl {
                    WriterCtrl::Flush(ack) => {
                        let r = drain_and_flush(
                            &mut log_file, &mut idx_file, &mut log_pos,
                            &mut write_rx, batch_size,
                            &mut batch, &mut log_buf, &mut idx_buf, &mut new_entries,
                            fsync, &index, &count,
                        ).await;
                        let _ = ack.send(r);
                    }
                    WriterCtrl::Shutdown(ack) => {
                        // Drain everything pending, fsync regardless of `fsync`
                        // option to maximize durability on shutdown.
                        let r = shutdown_drain(
                            &mut log_file, &mut idx_file, &mut log_pos,
                            &mut write_rx, batch_size,
                            &mut batch, &mut log_buf, &mut idx_buf, &mut new_entries,
                            &index, &count,
                        ).await;
                        let _ = ack.send(r);
                        // Notify any stragglers still in the queue so callers
                        // don't await forever.
                        while let Ok(p) = write_rx.try_recv() {
                            let _ = p.ack.send(Err(EventMeshError::Closed));
                        }
                        return;
                    }
                }
            }
            maybe = write_rx.recv() => {
                let Some(first) = maybe else { return };
                batch.push(first);
                while batch.len() < batch_size {
                    match write_rx.try_recv() {
                        Ok(p) => batch.push(p),
                        Err(_) => break,
                    }
                }
                if let Err(e) = write_batch(
                    &mut log_file, &mut idx_file, &mut log_pos,
                    &mut batch, &mut log_buf, &mut idx_buf,
                    &mut new_entries, fsync, &index, &count,
                ).await {
                    let msg = e.to_string();
                    for p in batch.drain(..) {
                        let _ = p.ack.send(Err(EventMeshError::InvalidTopic(
                            format!("write batch failed: {msg}"))));
                    }
                }
            }
            else => return,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn drain_and_flush(
    log_file: &mut File,
    idx_file: &mut File,
    log_pos: &mut u64,
    write_rx: &mut mpsc::Receiver<PendingWrite>,
    batch_size: usize,
    batch: &mut Vec<PendingWrite>,
    log_buf: &mut Vec<u8>,
    idx_buf: &mut Vec<u8>,
    new_entries: &mut Vec<IndexEntry>,
    fsync: bool,
    index: &Arc<TopicIndex>,
    count: &Arc<AtomicU64>,
) -> Result<()> {
    while batch.len() < batch_size {
        match write_rx.try_recv() {
            Ok(p) => batch.push(p),
            Err(_) => break,
        }
    }
    if !batch.is_empty() {
        write_batch(
            log_file, idx_file, log_pos, batch, log_buf, idx_buf,
            new_entries, fsync, index, count,
        )
        .await?;
    } else {
        log_file.flush().await?;
        idx_file.flush().await?;
        if fsync {
            log_file.sync_data().await?;
            idx_file.sync_data().await?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn shutdown_drain(
    log_file: &mut File,
    idx_file: &mut File,
    log_pos: &mut u64,
    write_rx: &mut mpsc::Receiver<PendingWrite>,
    batch_size: usize,
    batch: &mut Vec<PendingWrite>,
    log_buf: &mut Vec<u8>,
    idx_buf: &mut Vec<u8>,
    new_entries: &mut Vec<IndexEntry>,
    index: &Arc<TopicIndex>,
    count: &Arc<AtomicU64>,
) -> Result<()> {
    // Loop until queue is empty.
    loop {
        while batch.len() < batch_size {
            match write_rx.try_recv() {
                Ok(p) => batch.push(p),
                Err(_) => break,
            }
        }
        if batch.is_empty() {
            break;
        }
        write_batch(
            log_file, idx_file, log_pos, batch, log_buf, idx_buf,
            new_entries, true, index, count,
        )
        .await?;
    }
    log_file.flush().await?;
    idx_file.flush().await?;
    log_file.sync_all().await?;
    idx_file.sync_all().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn write_batch(
    log_file: &mut File,
    idx_file: &mut File,
    log_pos: &mut u64,
    batch: &mut Vec<PendingWrite>,
    log_buf: &mut Vec<u8>,
    idx_buf: &mut Vec<u8>,
    new_entries: &mut Vec<IndexEntry>,
    fsync: bool,
    index: &Arc<TopicIndex>,
    count: &Arc<AtomicU64>,
) -> Result<()> {
    log_buf.clear();
    idx_buf.clear();
    new_entries.clear();

    let mut cursor = *log_pos;
    for w in batch.iter() {
        let before = log_buf.len();
        encode_record(&w.env, log_buf)?;
        let len = (log_buf.len() - before) as u32;
        let topic = index.intern_topic(&w.env.topic);
        let ts_ms = w.env.timestamp.timestamp_millis();
        idx_buf.extend_from_slice(cursor.to_string().as_bytes());
        idx_buf.push(b'\t');
        idx_buf.extend_from_slice(len.to_string().as_bytes());
        idx_buf.push(b'\t');
        idx_buf.extend_from_slice(ts_ms.to_string().as_bytes());
        idx_buf.push(b'\t');
        idx_buf.extend_from_slice(topic.as_bytes());
        idx_buf.push(b'\n');
        new_entries.push(IndexEntry {
            offset: cursor,
            len,
            ts_ms,
            topic,
        });
        cursor += len as u64;
    }

    // Write log first, then idx — if we crash between the two writes the idx
    // is shorter than the log; recovery scans the gap and re-derives idx
    // entries from the log itself.
    log_file.write_all(log_buf).await?;
    log_file.flush().await?;
    if fsync {
        log_file.sync_data().await?;
    }
    idx_file.write_all(idx_buf).await?;
    idx_file.flush().await?;
    if fsync {
        idx_file.sync_data().await?;
    }

    index.extend(new_entries.drain(..));
    count.fetch_add(batch.len() as u64, Ordering::Release);
    *log_pos = cursor;

    for p in batch.drain(..) {
        let _ = p.ack.send(Ok(()));
    }
    Ok(())
}
