//! Metacognition 桥接（M3）：校准数据冷归档 + 检索。
//!
//! ## 真值源边界（ARCHITECTURE.md §6.3）
//!
//! - **派生层** `accumulated_pred_error: f32`（热路径读，零 IO）
//! - **事实层** = `CalibrationHistory`（内存环形缓冲，热）
//!   + `metacog/pred_errors/`（evict 后冷归档）
//! - `PredErrorSample ≡ agent-metacognition::CalibrationRecord`

use agent_context_db_core::{
    ContentLevel, ContentRepo, ContextEntry, ContextMeta, ContextUri, ContentType,
    FsOps, MvccVersion, Result, TenantId,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ===========================================================================
// PredErrorSample
// ===========================================================================

/// 预测误差样本 —— 与 agent-metacognition 的 `CalibrationRecord` 对齐。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredErrorSample {
    pub predicted_state_id: String,
    pub actual_state_id: String,
    /// 校准偏差值（越大越不准）。
    pub calibration: f32,
    /// 元认知评分。
    pub meta_score: f32,
    /// 时间戳（纳秒），冷归档文件名。
    pub ts: i64,
}

// ===========================================================================
// TimeWindow
// ===========================================================================

#[derive(Debug, Clone, Copy)]
pub struct TimeWindow {
    /// 起始时间戳（纳秒，闭区间）。
    pub from_ts: i64,
    /// 结束时间戳（纳秒，闭区间）。
    pub to_ts: i64,
}

impl TimeWindow {
    pub fn contains(&self, ts: i64) -> bool {
        ts >= self.from_ts && ts <= self.to_ts
    }
}

// ===========================================================================
// MetacogBridge
// ===========================================================================

pub struct MetacogBridge<S> {
    store: Arc<S>,
}

impl<S: FsOps + ContentRepo> MetacogBridge<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// `metacog/pred_errors/` 目录 URI。
    fn pred_errors_dir(agent_id: &str) -> ContextUri {
        ContextUri(format!(
            "uwu://default/agent/{}/metacog/pred_errors",
            agent_id
        ))
    }

    fn sample_uri(agent_id: &str, ts: i64) -> ContextUri {
        Self::pred_errors_dir(agent_id).join(&format!("{}.json", ts))
    }

    // ── log_pred_error ───────────────────────────────────────────

    /// 归档：CalibrationHistory evict 出的记录落盘为冷存。
    ///
    /// 热路径写的是内存环形缓冲，此方法仅在 evict 时触发。
    pub async fn log_pred_error(
        &self,
        agent_id: &str,
        sample: &PredErrorSample,
        tenant: TenantId,
    ) -> Result<MvccVersion> {
        let uri = Self::sample_uri(agent_id, sample.ts);
        let json = serde_json::to_string(sample)
            .unwrap_or_else(|_| "{}".to_string());

        let entry = ContextEntry {
            uri,
            tenant,
            l0_abstract: format!(
                "pred_error cal={:.4} meta={:.4} ts={}",
                sample.calibration, sample.meta_score, sample.ts
            ),
            l1_overview: Some(json.clone()),
            l2_detail_uri: None,
            content_type: ContentType::Text,
            metadata: ContextMeta::default(),
            mvcc_version: MvccVersion(0),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.store.write(entry).await
    }

    // ── retrieve_calibration ─────────────────────────────────────

    /// 检索历史校准数据：合并内存（热，未 evict）与 FS（冷，已归档）两处。
    ///
    /// `hot_samples` 为调用方传入的内存环形缓冲中的记录（未 evict 部分）。
    /// 方法先取 FS 中窗口内归档文件，再合并热数据（热优先去重）。
    pub async fn retrieve_calibration(
        &self,
        agent_id: &str,
        window: TimeWindow,
        hot_samples: &[PredErrorSample],
    ) -> Result<Vec<PredErrorSample>> {
        let dir = Self::pred_errors_dir(agent_id);

        // 1. 从 FS 读取窗口内冷归档
        let mut samples: Vec<PredErrorSample> = Vec::new();
        let mut seen_ts = std::collections::HashSet::new();

        match self.store.ls(&dir).await {
            Ok(entries) => {
                for entry in entries {
                    if entry.is_dir {
                        continue;
                    }
                    // 从文件名提取 ts：{ts}.json
                    let uri_str = entry.uri.to_string();
                    let file_name = uri_str.rsplit('/').next().unwrap_or("");
                    let ts_str = file_name.strip_suffix(".json").unwrap_or("");
                    if let Ok(ts) = ts_str.parse::<i64>() {
                        if window.contains(ts) {
                            match self.store.read(&entry.uri, ContentLevel::L1).await {
                                Ok(content) => {
                                    let json_str = match &content {
                                        agent_context_db_core::ContentPayload::Overview(s) => s.clone(),
                                        agent_context_db_core::ContentPayload::Abstract(s) => s.clone(),
                                        _ => continue,
                                    };
                                    if let Ok(sample) = serde_json::from_str::<PredErrorSample>(&json_str) {
                                        seen_ts.insert(sample.ts);
                                        samples.push(sample);
                                    }
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // 目录不存在 → 没有冷归档，继续
            }
        }

        // 2. 合并热数据（同一 ts 用热数据覆盖冷数据）
        for hot in hot_samples {
            if window.contains(hot.ts) {
                if seen_ts.contains(&hot.ts) {
                    // 替换冷数据中同 ts 的旧版本
                    samples.retain(|s| s.ts != hot.ts);
                }
                samples.push(hot.clone());
            }
        }

        // 3. 按 ts 排序
        samples.sort_by_key(|s| s.ts);

        Ok(samples)
    }
}
