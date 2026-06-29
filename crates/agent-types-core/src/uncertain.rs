//! Uncertain<T> — value with confidence

use serde::{Deserialize, Serialize};

/// 带置信度的值，置信度范围 [0.0, 1.0]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Uncertain<T> {
    pub value: T,
    pub confidence: f32,
}

impl<T> Uncertain<T> {
    pub fn new(value: T, confidence: f32) -> Self {
        Self {
            value,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// 置信度是否高于阈值
    pub fn is_confident(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}
