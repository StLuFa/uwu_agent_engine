//! Embedding —— 向量嵌入

use serde::{Deserialize, Serialize};

/// 向量嵌入 —— f32 数组的薄包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    pub dim: usize,
    pub values: Vec<f32>,
}

impl Embedding {
    pub fn new(values: Vec<f32>) -> Self {
        Self {
            dim: values.len(),
            values,
        }
    }

    /// 计算两个嵌入的余弦相似度
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
    }

    /// 简单的伪嵌入生成（开发用）—— 基于文本哈希的确定性向量
    ///
    /// 生产环境应使用外部 embedding 服务（OpenAI/text-embedding-3-small 等）。
    pub fn mock(text: &str, dim: usize) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut values = Vec::with_capacity(dim);
        for i in 0..dim {
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            i.hash(&mut hasher);
            let h = hasher.finish();
            // Map u64 to [-1.0, 1.0]
            values.push((h as f32 / u64::MAX as f32) * 2.0 - 1.0);
        }
        // Normalize
        let norm: f32 = values.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut values {
                *v /= norm;
            }
        }
        Self::new(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        let sim = Embedding::cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = Embedding::cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn mock_embedding_same_text_produces_same_vector() {
        let e1 = Embedding::mock("hello", 16);
        let e2 = Embedding::mock("hello", 16);
        assert_eq!(e1.values, e2.values);
    }
}
