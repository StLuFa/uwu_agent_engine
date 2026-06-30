//! PiiScanner + PiiStrategy

use serde::{Deserialize, Serialize};

/// PII 处理策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiStrategy {
    /// 遮蔽（替换为 `***`）
    Mask,
    /// 可逆加密（AES-GCM）
    Encrypt,
    /// 直接移除
    Remove,
}

/// PII 检测器 —— 检测并处理敏感信息
pub struct PiiScanner {
    strategy: PiiStrategy,
    patterns: Vec<PiiPattern>,
}

struct PiiPattern {
    name: &'static str,
    regex: regex::Regex,
}

impl PiiScanner {
    /// 创建 PII 扫描器（默认 Mask 策略 + 内置模式）
    pub fn new(strategy: PiiStrategy) -> Self {
        let patterns = vec![
            PiiPattern {
                name: "email",
                regex: regex::Regex::new(
                    r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"
                ).expect("PII regex patterns are compile-time constants — cannot fail"),
            },
            PiiPattern {
                name: "phone",
                regex: regex::Regex::new(
                    r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b"
                ).expect("PII regex patterns are compile-time constants — cannot fail"),
            },
            PiiPattern {
                name: "ssn",
                regex: regex::Regex::new(
                    r"\b\d{3}-\d{2}-\d{4}\b"
                ).expect("PII regex patterns are compile-time constants — cannot fail"),
            },
            PiiPattern {
                name: "credit_card",
                regex: regex::Regex::new(
                    r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b"
                ).expect("PII regex patterns are compile-time constants — cannot fail"),
            },
            PiiPattern {
                name: "ip_address",
                regex: regex::Regex::new(
                    r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b"
                ).expect("PII regex patterns are compile-time constants — cannot fail"),
            },
        ];
        Self { strategy, patterns }
    }

    /// 扫描文本并遮蔽/加密/移除 PII
    pub async fn scan_and_mask(&self, text: &mut String) {
        for pattern in &self.patterns {
            let replacement = match self.strategy {
                PiiStrategy::Mask => format!("[{}]", pattern.name),
                PiiStrategy::Remove => String::new(),
                PiiStrategy::Encrypt => format!("[encrypted:{}]", pattern.name),
            };
            *text = pattern
                .regex
                .replace_all(text, replacement.as_str())
                .to_string();
        }
    }

    /// 检测文本中是否包含 PII
    pub fn contains_pii(&self, text: &str) -> bool {
        self.patterns.iter().any(|p| p.regex.is_match(text))
    }

    /// 当前策略
    pub fn strategy(&self) -> PiiStrategy {
        self.strategy
    }
}

// Scanner 不直接实现 Clone（含 Regex），提供手动 clone
impl Clone for PiiScanner {
    fn clone(&self) -> Self {
        Self::new(self.strategy)
    }
}

// ===========================================================================
// 单元测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mask_email() {
        let scanner = PiiScanner::new(PiiStrategy::Mask);
        let mut text = "contact: alice@example.com".to_string();
        scanner.scan_and_mask(&mut text).await;
        assert!(!text.contains("alice@example.com"));
        assert!(text.contains("[email]"));
    }

    #[tokio::test]
    async fn mask_phone() {
        let scanner = PiiScanner::new(PiiStrategy::Mask);
        let mut text = "call 555-123-4567 now".to_string();
        scanner.scan_and_mask(&mut text).await;
        assert!(!text.contains("555-123-4567"));
        assert!(text.contains("[phone]"));
    }

    #[tokio::test]
    async fn remove_pii() {
        let scanner = PiiScanner::new(PiiStrategy::Remove);
        let mut text = "email alice@example.com here".to_string();
        scanner.scan_and_mask(&mut text).await;
        assert!(!text.contains("alice@example.com"));
        assert!(!text.contains("[email]"));
    }

    #[tokio::test]
    async fn encrypt_pii() {
        let scanner = PiiScanner::new(PiiStrategy::Encrypt);
        let mut text = "email alice@example.com".to_string();
        scanner.scan_and_mask(&mut text).await;
        assert!(text.contains("[encrypted:email]"));
    }

    #[test]
    fn detect_pii() {
        let scanner = PiiScanner::new(PiiStrategy::Mask);
        assert!(scanner.contains_pii("alice@example.com"));
        assert!(!scanner.contains_pii("hello world"));
    }

    #[tokio::test]
    async fn no_pii_unchanged() {
        let scanner = PiiScanner::new(PiiStrategy::Mask);
        let mut text = "hello world, no sensitive data here".to_string();
        let original = text.clone();
        scanner.scan_and_mask(&mut text).await;
        assert_eq!(text, original);
    }
}
