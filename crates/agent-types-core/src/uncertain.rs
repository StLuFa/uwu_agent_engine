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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clamps_confidence() {
        let u = Uncertain::new("test", 1.5);
        assert_eq!(u.confidence, 1.0);
        let u = Uncertain::new("test", -0.5);
        assert_eq!(u.confidence, 0.0);
    }

    #[test]
    fn new_stores_value() {
        let u = Uncertain::new(42u32, 0.8);
        assert_eq!(u.value, 42);
        assert_eq!(u.confidence, 0.8);
    }

    #[test]
    fn is_confident_at_threshold() {
        let u = Uncertain::new("x", 0.9);
        assert!(u.is_confident(0.8));
        assert!(u.is_confident(0.9), "boundary: confidence == threshold should pass");
        assert!(!u.is_confident(0.95));
    }

    #[test]
    fn is_confident_edge_cases() {
        let u = Uncertain::new("x", 1.0);
        assert!(u.is_confident(1.0));
        let u = Uncertain::new("x", 0.0);
        assert!(!u.is_confident(0.5));
    }

    #[test]
    fn serde_roundtrip() {
        let u = Uncertain::new("hello", 0.75);
        let json = serde_json::to_string(&u).unwrap();
        let decoded: Uncertain<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.value, "hello");
        assert!((decoded.confidence - 0.75).abs() < 0.001);
    }

    #[test]
    fn serde_roundtrip_numeric() {
        let u = Uncertain::new(3.14f64, 0.5);
        let json = serde_json::to_string(&u).unwrap();
        let decoded: Uncertain<f64> = serde_json::from_str(&json).unwrap();
        assert!((decoded.value - 3.14).abs() < 0.001);
        assert!((decoded.confidence - 0.5).abs() < 0.001);
    }
}
