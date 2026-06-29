//! OutputFormatter

use serde::{Deserialize, Serialize};

/// 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    /// 纯文本
    PlainText,
    /// JSON
    Json,
    /// Markdown
    Markdown,
}

/// 输出格式化器
pub struct OutputFormatter {
    format: OutputFormat,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// 格式化输出文本
    pub fn format(&self, content: &str) -> String {
        match self.format {
            OutputFormat::PlainText => content.to_string(),
            OutputFormat::Json => {
                serde_json::json!({"content": content}).to_string()
            }
            OutputFormat::Markdown => {
                format!("> {}\n", content.replace('\n', "\n> "))
            }
        }
    }

    /// 格式化执行结果为人类可读文本
    pub fn format_result(
        &self,
        action_command: &str,
        success: bool,
        output: &str,
        time_ms: u64,
    ) -> String {
        let status = if success { "OK" } else { "FAILED" };
        match self.format {
            OutputFormat::PlainText => {
                format!("[{status}] {action_command} ({time_ms}ms): {output}")
            }
            OutputFormat::Json => {
                serde_json::json!({
                    "status": status,
                    "action": action_command,
                    "output": output,
                    "time_ms": time_ms
                })
                .to_string()
            }
            OutputFormat::Markdown => {
                format!("**`{action_command}`** [{status}] ({time_ms}ms)\n\n{output}\n")
            }
        }
    }

    pub fn current_format(&self) -> OutputFormat {
        self.format
    }
}

impl Default for OutputFormatter {
    fn default() -> Self {
        Self::new(OutputFormat::PlainText)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_format() {
        let formatter = OutputFormatter::new(OutputFormat::PlainText);
        let result = formatter.format_result("click", true, "clicked", 50);
        assert!(result.contains("[OK]"));
        assert!(result.contains("click"));
    }

    #[test]
    fn json_format() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let result = formatter.format_result("search", true, "found 5", 100);
        assert!(result.contains("\"status\""));
        assert!(result.contains("\"OK\""));
    }

    #[test]
    fn markdown_format() {
        let formatter = OutputFormatter::new(OutputFormat::Markdown);
        let result = formatter.format("hello\nworld");
        assert!(result.contains("> hello"));
        assert!(result.contains("> world"));
    }
}
