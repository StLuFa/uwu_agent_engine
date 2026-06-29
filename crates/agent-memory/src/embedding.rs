//! Embedding —— 向量嵌入（bigram 随机投影）
//!
//! 使用字符 bigram 特征 + 确定性随机投影矩阵生成嵌入。
//! 相似文本共享 bigram → 向量距离小，优于纯哈希方法。
//! 零外部依赖，纯 Rust 数学。

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

    /// 生成确定性嵌入向量。
    ///
    /// 算法：提取字符 bigram → 哈希到稀疏特征 → 随机投影 → L2 归一化。
    /// - 相同文本 → 相同向量
    /// - 相似文本（共享 bigram）→ 余弦相似度 > 0.5
    /// - 无关文本 → 余弦相似度 ≈ 0.0
    ///
    /// 生产环境可替换为外部 embedding 服务。
    pub fn mock(text: &str, dim: usize) -> Self {
        if text.is_empty() || dim == 0 {
            return Self::new(vec![0.0; dim]);
        }

        // 1. 提取字符 bigram（滑动窗口）
        let chars: Vec<char> = text.chars().collect();
        let bigrams: Vec<String> = if chars.len() >= 2 {
            chars
                .windows(2)
                .map(|w| format!("{}{}", w[0], w[1]))
                .collect()
        } else {
            vec![text.to_string()]
        };

        // 2. 每个 bigram → 特征索引（模 FEATURE_SPACE）
        const FEATURE_SPACE: usize = 4096;
        let mut features = vec![0.0f32; FEATURE_SPACE];
        for bg in &bigrams {
            let idx = hash_bigram(bg) as usize % FEATURE_SPACE;
            // TF-like: 出现次数越多，该特征越强
            features[idx] += 1.0;
        }

        // 3. 稀疏特征 → 稠密嵌入（随机投影矩阵 R: FEATURE_SPACE × dim）
        //    每列是独立的标准正态随机数（确定性，种子固定）
        let mut values = vec![0.0f32; dim];
        for (j, val) in values.iter_mut().enumerate() {
            let mut dot = 0.0f32;
            for (i, feat) in features.iter().enumerate() {
                if *feat > 0.0 {
                    dot += feat * pseudo_gaussian(i, j);
                }
            }
            *val = dot;
        }

        // 4. 归一化
        let norm: f32 = values.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut values {
                *v /= norm;
            }
        } else {
            values[0] = 1.0;
        }

        Self::new(values)
    }
}

/// 确定性哈希：bigram → u64
fn hash_bigram(bigram: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    bigram.hash(&mut hasher);
    hasher.finish()
}

/// 伪标准正态随机数：给定 (row, col) → 近似 N(0,1)。
/// 使用 Box-Muller 变换 + 确定性种子。
fn pseudo_gaussian(i: usize, j: usize) -> f32 {
    // 确定性种子：混合行和列索引
    let seed = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15u64)
        ^ (j as u64).wrapping_mul(0xC6A4_A793_5BD1_E995u64);
    // SplitMix64 → [0, 1)
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9).wrapping_add(x >> 32);
    let u = (x >> 32) as f32 / (u32::MAX as f32);
    // 第二个随机数（固定偏移）
    let mut y = x.wrapping_add(0x94D0_49BB_1331_11EB);
    y = y.wrapping_mul(0xBF58_476D_1CE4_E5B9).wrapping_add(y >> 32);
    let v = (y >> 32) as f32 / (u32::MAX as f32);
    // Box-Muller: Z = sqrt(-2 ln u) * cos(2π v)
    let u_safe = u.max(1e-10);
    (-2.0 * u_safe.ln()).sqrt() * (2.0 * std::f32::consts::PI * v).cos()
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

    #[test]
    fn similar_texts_have_higher_similarity() {
        let e1 = Embedding::mock("rust programming language guide", 32);
        let e2 = Embedding::mock("rust programming language tutorial", 32);
        let e3 = Embedding::mock("banana smoothie recipe", 32);

        let sim_similar = Embedding::cosine_similarity(&e1.values, &e2.values);
        let sim_diff = Embedding::cosine_similarity(&e1.values, &e3.values);

        assert!(
            sim_similar > sim_diff,
            "similar texts should be closer: {sim_similar:.3} vs {sim_diff:.3}"
        );
    }

    #[test]
    fn empty_text_produces_zero_vector() {
        let e = Embedding::mock("", 8);
        assert_eq!(e.values, vec![0.0; 8]);
    }

    #[test]
    fn single_char_text_works() {
        let e = Embedding::mock("a", 16);
        assert_eq!(e.values.len(), 16);
    }
}
