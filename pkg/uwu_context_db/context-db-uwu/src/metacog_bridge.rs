//! Metacognition 桥接（M3 骨架）：校准数据冷归档 + 检索。
//!
//! 见 ARCHITECTURE.md §6.3 真值源边界：
//! - **派生层** `accumulated_pred_error: f32`（热路径读，零 IO）
//! - **事实层** = `CalibrationHistory`（内存环形缓冲，热）+ `metacog/pred_errors/`（evict 后冷归档）
//! - `PredErrorSample ≡ agent-metacognition::CalibrationRecord`

use agent_context_db_core::{ContentRepo, FsOps, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 预测误差样本 —— 与 agent-metacognition 的 `CalibrationRecord` 对齐（不另造范式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredErrorSample {
    pub predicted_state_id: String,
    pub actual_state_id: String,
    pub calibration: f32,
    pub meta_score: f32,
    /// 时间戳（纳秒），冷归档文件名用它。
    pub ts: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeWindow {
    pub from_ts: i64,
    pub to_ts: i64,
}

pub struct MetacogBridge<S: FsOps + ContentRepo> {
    store: Arc<S>,
}

impl<S: FsOps + ContentRepo> MetacogBridge<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// 归档：CalibrationHistory evict 出的记录落盘为冷存。
    /// 热路径写的是内存环形缓冲，不是这里；此方法仅在 evict 时触发。
    pub async fn log_pred_error(&self, _agent_id: &str, _sample: &PredErrorSample) -> Result<()> {
        // write ContextEntry -> uwu://.../metacog/pred_errors/{ts}.json
        unimplemented!("M3 对接 agent-metacognition evict 钩子时实现")
    }

    /// 检索历史校准数据：合并内存（热，未 evict）与 FS（冷，已归档）两处。
    pub async fn retrieve_calibration(
        &self,
        _agent_id: &str,
        _window: TimeWindow,
    ) -> Result<Vec<PredErrorSample>> {
        // 1. 内存 CalibrationHistory 窗口内记录（热）
        // 2. ls metacog/pred_errors/ 窗口内归档（冷），并行 read(L2) 反序列化
        // 3. 按 ts 合并去重，热优先
        unimplemented!("M3 对接时实现")
    }
}
