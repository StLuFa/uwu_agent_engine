//! ConfidenceMap —— 事实/假设置信度映射

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 从 fact/hypothesis key 到置信度的映射
///
/// 附加在 AgentState 上，追踪对各项知识的置信度。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfidenceMap {
    entries: HashMap<String, f32>,
}

impl ConfidenceMap {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// 获取某项的置信度，不存在则返回 0.0
    pub fn get(&self, key: &str) -> f32 {
        self.entries.get(key).copied().unwrap_or(0.0)
    }

    /// 设置置信度（自动 clamp 到 [0.0, 1.0]）
    pub fn set(&mut self, key: impl Into<String>, confidence: f32) {
        self.entries.insert(key.into(), confidence.clamp(0.0, 1.0));
    }

    /// 移除某项
    pub fn remove(&mut self, key: &str) {
        self.entries.remove(key);
    }

    /// 条目数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 所有条目的平均置信度
    pub fn average(&self) -> f32 {
        if self.entries.is_empty() {
            return 0.0;
        }
        self.entries.values().sum::<f32>() / self.entries.len() as f32
    }

    /// 遍历所有 (key, confidence) 对
    pub fn iter(&self) -> impl Iterator<Item = (&String, &f32)> {
        self.entries.iter()
    }
}
