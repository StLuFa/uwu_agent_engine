//! CalibrationRecord + CalibrationHistory

use crate::calibrate::CalibrationResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 一条校准记录 —— 记录一次 evaluate() 的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationRecord {
    /// 预测状态的 ID
    pub predicted_state_id: String,
    /// 实际状态的 ID（calibrate_with_outcome 时填入）
    pub actual_state_id: Option<String>,
    /// 校准结果
    pub calibration: CalibrationResult,
    /// 元分数
    pub meta_score: f32,
    /// 记录时间
    pub timestamp: DateTime<Utc>,
}

/// 校准历史 —— 环形缓冲，保留最近 N 条记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationHistory {
    records: VecDeque<CalibrationRecord>,
    /// 最大容量
    capacity: usize,
}

impl CalibrationHistory {
    /// 创建历史缓冲，默认容量 1000
    pub fn new(capacity: usize) -> Self {
        Self {
            records: VecDeque::with_capacity(capacity.min(10000)),
            capacity: capacity.min(10000),
        }
    }

    /// 追加一条记录，超容量时弹出最旧的
    pub fn push(&mut self, record: CalibrationRecord) {
        if self.records.len() >= self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }

    /// 获取最近 n 条记录（按时间从旧到新）
    pub fn recent(&self, n: usize) -> Vec<&CalibrationRecord> {
        self.records
            .iter()
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// 当前记录数量
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// 所有记录的迭代器
    pub fn iter(&self) -> impl Iterator<Item = &CalibrationRecord> {
        self.records.iter()
    }

    /// 获取最近的 meta_score 平均值
    pub fn recent_avg_meta_score(&self, n: usize) -> f32 {
        let recent = self.recent(n);
        if recent.is_empty() {
            return 0.0;
        }
        recent.iter().map(|r| r.meta_score).sum::<f32>() / recent.len() as f32
    }
}

impl Default for CalibrationHistory {
    fn default() -> Self {
        Self::new(1000)
    }
}
