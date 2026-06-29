//! Per-topic secondary index for [`super::JsonlStore`], with **CRC-validated
//! crash recovery** on open.
//!
//! ## On-disk record format
//!
//! Each log line is `<crc32_hex>\t<json>\n` where `crc32_hex` is the
//! hex-encoded CRC32 of the JSON portion (8 lowercase hex chars). On open we
//! validate every newly-discovered record's CRC; corrupt or torn-write
//! tails cause the log to be truncated to the last good record.
//!
//! ## Index file format
//!
//! `<offset>\t<len>\t<ts_ms>\t<topic>\n` — one line per record. `ts_ms` is
//! the envelope timestamp in unix milliseconds; the in-memory index uses
//! it to prune by time during `query()`. Older index files written without
//! a timestamp (3-field form) are still accepted on load.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};

use crate::core::envelope::Envelope;
use crate::core::error::Result;

#[derive(Debug, Clone)]
pub(crate) struct IndexEntry {
    pub offset: u64,
    pub len: u32,
    /// Envelope timestamp in unix milliseconds (signed to match chrono).
    pub ts_ms: i64,
    pub topic: Arc<str>,
}

#[derive(Default)]
pub(crate) struct TopicIndex {
    by_topic: RwLock<HashMap<Arc<str>, Vec<IndexEntry>>>,
    intern: RwLock<HashMap<String, Arc<str>>>,
}

impl TopicIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern_topic(&self, topic: &str) -> Arc<str> {
        if let Some(s) = self.intern.read().get(topic) {
            return s.clone();
        }
        let mut w = self.intern.write();
        if let Some(s) = w.get(topic) {
            return s.clone();
        }
        let arc: Arc<str> = Arc::from(topic);
        w.insert(topic.to_string(), arc.clone());
        arc
    }

    #[allow(dead_code)]
    pub fn push(&self, entry: IndexEntry) {
        self.by_topic
            .write()
            .entry(entry.topic.clone())
            .or_default()
            .push(entry);
    }

    pub fn extend(&self, entries: impl IntoIterator<Item = IndexEntry>) {
        let mut by_topic = self.by_topic.write();
        for e in entries {
            by_topic.entry(e.topic.clone()).or_default().push(e);
        }
    }

    pub fn len(&self) -> usize {
        self.by_topic.read().values().map(|v| v.len()).sum()
    }

    #[allow(dead_code)]
    pub fn snapshot_filtered<F>(&self, mut topic_pred: F) -> Vec<IndexEntry>
    where
        F: FnMut(&str) -> bool,
    {
        let by_topic = self.by_topic.read();
        let mut out = Vec::new();
        for (topic, entries) in by_topic.iter() {
            if topic_pred(topic) {
                out.extend(entries.iter().cloned());
            }
        }
        out.sort_by_key(|e| e.offset);
        out
    }

    /// Topic + time-range pre-filter. `since_ms` / `until_ms` are inclusive
    /// when `Some` and unbounded when `None`. Entries with `ts_ms == 0`
    /// (loaded from a legacy index without timestamps) bypass the time
    /// filter and are emitted; the full filter on the envelope itself will
    /// reject them later if they don't match.
    pub fn snapshot_filtered_time<F>(
        &self,
        mut topic_pred: F,
        since_ms: Option<i64>,
        until_ms: Option<i64>,
    ) -> Vec<IndexEntry>
    where
        F: FnMut(&str) -> bool,
    {
        let by_topic = self.by_topic.read();
        let mut out = Vec::new();
        for (topic, entries) in by_topic.iter() {
            if !topic_pred(topic) {
                continue;
            }
            for e in entries {
                if e.ts_ms != 0 {
                    if let Some(s) = since_ms {
                        if e.ts_ms < s {
                            continue;
                        }
                    }
                    if let Some(u) = until_ms {
                        if e.ts_ms > u {
                            continue;
                        }
                    }
                }
                out.push(e.clone());
            }
        }
        out.sort_by_key(|e| e.offset);
        out
    }
}

// ============================================================================
// Record codec — `<crc32_hex>\t<json>\n`
// ============================================================================

/// CRC + tab prefix length in bytes (8 hex + 1 tab).
pub(crate) const RECORD_PREFIX_LEN: usize = 9;

/// Encode an envelope record into `out` and return the JSON-portion length.
/// Layout: `<8 hex crc><tab><json><lf>`.
pub(crate) fn encode_record(env: &Envelope, out: &mut Vec<u8>) -> Result<()> {
    // First serialize json into a scratch slice we can checksum.
    let start = out.len();
    out.extend_from_slice(b"00000000\t");
    let json_start = out.len();
    serde_json::to_writer(&mut *out, env)?;
    out.push(b'\n');
    let json_end = out.len() - 1; // exclude '\n'
    let crc = crc32fast::hash(&out[json_start..json_end]);
    let hex = format!("{crc:08x}");
    debug_assert_eq!(hex.len(), 8);
    out[start..start + 8].copy_from_slice(hex.as_bytes());
    Ok(())
}

/// Validate one full record line (including trailing `\n`). Returns the JSON
/// byte slice on success.
fn verify_record(line: &[u8]) -> Option<&[u8]> {
    if line.last() != Some(&b'\n') {
        return None;
    }
    if line.len() <= RECORD_PREFIX_LEN {
        return None;
    }
    if line[8] != b'\t' {
        return None;
    }
    let crc_hex = std::str::from_utf8(&line[..8]).ok()?;
    let expected = u32::from_str_radix(crc_hex, 16).ok()?;
    let json = &line[RECORD_PREFIX_LEN..line.len() - 1];
    if crc32fast::hash(json) != expected {
        return None;
    }
    Some(json)
}

// ============================================================================
// Recovery / load
// ============================================================================

/// Result of opening a JsonlStore:
/// - in-memory index populated
/// - writer-side cursor (file offset to append from)
pub(crate) struct LoadOutcome {
    pub index: TopicIndex,
    pub log_pos: u64,
    /// Entries discovered by tail-recovery that must still be appended to the
    /// `.idx` file. The writer task does this on first batch flush via the
    /// `pending_idx_writes` field on `JsonlStore`.
    pub pending_idx_appends: Vec<IndexEntry>,
}

pub(crate) async fn load_and_recover(
    log_path: &Path,
    idx_path: &Path,
) -> Result<LoadOutcome> {
    let index = TopicIndex::new();
    let log_size = if log_path.exists() {
        tokio::fs::metadata(log_path).await?.len()
    } else {
        0
    };

    // ---- 1. Try to load idx ------------------------------------------------
    let mut idx_end: u64 = 0;
    let mut idx_usable = false;
    if idx_path.exists() {
        let f = File::open(idx_path).await?;
        let mut lines = BufReader::new(f).lines();
        let mut entries: Vec<IndexEntry> = Vec::new();
        while let Some(line) = lines.next_line().await? {
            // Accept both legacy 3-field form (offset\tlen\ttopic) and the
            // current 4-field form (offset\tlen\tts_ms\ttopic).
            let parts: Vec<&str> = line.splitn(4, '\t').collect();
            let (offset, len, ts_ms, topic) = match parts.len() {
                4 => {
                    let Ok(offset) = parts[0].parse::<u64>() else { continue };
                    let Ok(len) = parts[1].parse::<u32>() else { continue };
                    let ts_ms = parts[2].parse::<i64>().unwrap_or(0);
                    (offset, len, ts_ms, parts[3])
                }
                3 => {
                    let Ok(offset) = parts[0].parse::<u64>() else { continue };
                    let Ok(len) = parts[1].parse::<u32>() else { continue };
                    (offset, len, 0i64, parts[2])
                }
                _ => continue,
            };
            let topic = index.intern_topic(topic);
            entries.push(IndexEntry { offset, len, ts_ms, topic });
        }

        // Stale: idx references bytes past current log size → discard.
        let claims_past_log = entries
            .last()
            .map(|e| e.offset + e.len as u64 > log_size)
            .unwrap_or(false);

        if claims_past_log {
            let _ = tokio::fs::remove_file(idx_path).await;
        } else {
            // Validate the last referenced record's CRC; if torn → discard idx.
            let last_ok = match entries.last() {
                None => true,
                Some(last) => validate_record_at(log_path, last.offset, last.len)
                    .await
                    .unwrap_or(false),
            };
            if !last_ok {
                let _ = tokio::fs::remove_file(idx_path).await;
            } else {
                idx_end = entries.last().map(|e| e.offset + e.len as u64).unwrap_or(0);
                index.extend(entries);
                idx_usable = true;
            }
        }
    }

    if !idx_usable {
        // Drop any partial in-memory state (shouldn't have any since we only
        // pushed on the success path), then fall through to a full tail scan
        // from offset 0.
        idx_end = 0;
    }

    // ---- 2. Tail scan from idx_end, validating CRC -------------------------
    let (recovered, recovered_end) = scan_validate(log_path, idx_end, &index).await?;

    // ---- 3. Truncate log if a torn record was found ------------------------
    if recovered_end < log_size {
        let f = OpenOptions::new()
            .write(true)
            .open(log_path)
            .await?;
        f.set_len(recovered_end).await?;
        f.sync_all().await?;
    }

    // ---- 4. Push recovered entries into the in-memory index ---------------
    let pending_idx_appends = recovered.clone();
    index.extend(recovered);

    Ok(LoadOutcome {
        index,
        log_pos: recovered_end,
        pending_idx_appends,
    })
}

async fn validate_record_at(log_path: &Path, offset: u64, len: u32) -> Result<bool> {
    let mut f = File::open(log_path).await?;
    f.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut buf = vec![0u8; len as usize];
    if f.read_exact(&mut buf).await.is_err() {
        return Ok(false);
    }
    Ok(verify_record(&buf).is_some())
}

/// Sequentially read records from `start`, verifying CRC. Returns the entries
/// recovered and the offset of the first byte that did NOT pass validation
/// (i.e. the new end-of-log).
async fn scan_validate(
    log_path: &Path,
    start: u64,
    index: &TopicIndex,
) -> Result<(Vec<IndexEntry>, u64)> {
    if !log_path.exists() {
        return Ok((Vec::new(), start));
    }
    let mut f = File::open(log_path).await?;
    f.seek(std::io::SeekFrom::Start(start)).await?;
    let mut reader = BufReader::new(f);

    let mut out: Vec<IndexEntry> = Vec::new();
    let mut offset = start;
    let mut line: Vec<u8> = Vec::with_capacity(512);

    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line).await?;
        if n == 0 {
            break;
        }
        // Torn write: missing trailing newline.
        let Some(json) = verify_record(&line) else {
            break;
        };
        let env: Envelope = match serde_json::from_slice(json) {
            Ok(e) => e,
            Err(_) => break,
        };
        let topic = index.intern_topic(&env.topic);
        let ts_ms = env.timestamp.timestamp_millis();
        out.push(IndexEntry {
            offset,
            len: n as u32,
            ts_ms,
            topic,
        });
        offset += n as u64;
    }
    Ok((out, offset))
}

/// Append already-recovered entries to the `.idx` file. Used right after open
/// so the index is consistent with the log on disk.
pub(crate) async fn append_idx_entries(
    idx_path: &Path,
    entries: &[IndexEntry],
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(idx_path)
        .await?;
    let mut buf = Vec::with_capacity(entries.len() * 40);
    for e in entries {
        buf.extend_from_slice(e.offset.to_string().as_bytes());
        buf.push(b'\t');
        buf.extend_from_slice(e.len.to_string().as_bytes());
        buf.push(b'\t');
        buf.extend_from_slice(e.ts_ms.to_string().as_bytes());
        buf.push(b'\t');
        buf.extend_from_slice(e.topic.as_bytes());
        buf.push(b'\n');
    }
    f.write_all(&buf).await?;
    f.flush().await?;
    Ok(())
}
