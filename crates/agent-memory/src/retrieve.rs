//! RetrievalIntent + retrieve logic

use serde::{Deserialize, Serialize};

/// 检索意图 —— 描述"想要什么样的记忆"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalIntent {
    /// 查询文本
    pub query: String,
    /// 期望的记忆类型（None = 全部类型）
    pub preferred_types: Option<Vec<super::types::MemoryType>>,
    /// 返回结果上限
    pub max_results: usize,
    /// 相似度阈值
    pub min_similarity: f32,
}

impl RetrievalIntent {
    /// 简单查询 —— 全部类型，返回 10 条，阈值 0.0
    pub fn simple(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            preferred_types: None,
            max_results: 10,
            min_similarity: 0.0,
        }
    }

    /// 按类型查询
    pub fn typed(
        query: impl Into<String>,
        types: Vec<super::types::MemoryType>,
    ) -> Self {
        Self {
            query: query.into(),
            preferred_types: Some(types),
            max_results: 10,
            min_similarity: 0.0,
        }
    }

    pub fn with_max(mut self, max: usize) -> Self {
        self.max_results = max;
        self
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.min_similarity = threshold.clamp(0.0, 1.0);
        self
    }
}
