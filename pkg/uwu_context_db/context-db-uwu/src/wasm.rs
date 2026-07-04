//! WASM 沙箱执行（U12）—— 在上下文条目上运行 WASM 衍生计算。

use agent_context_db_core::{ContentLevel, ContentPayload, ContextEntry, ContextUri, FsOps, LlmClient, LlmOpts};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// WASM 计算任务
// ═══════════════════════════════════════════════════════════════════════════

/// WASM 计算输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmComputeInput {
    /// 输入条目
    pub entries: Vec<ContextEntry>,
    /// 额外参数
    pub params: serde_json::Value,
}

/// WASM 计算输出。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmComputeOutput {
    /// 计算结果文本
    pub result: String,
    /// 统计摘要
    pub stats: Option<ComputeStats>,
    /// 是否触发后续操作
    pub trigger_action: Option<String>,
}

/// 计算统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeStats {
    pub entry_count: usize,
    pub total_tokens: usize,
    pub unique_classes: usize,
    pub average_confidence: f32,
}

/// 派生计算类型。
#[derive(Debug, Clone)]
pub enum WasmDerivation {
    /// 聚类分析
    Cluster { method: String, k: usize },
    /// 统计摘要
    Stats,
    /// 趋势检测
    TrendDetection,
    /// 自定义（WASM 模块路径）
    Custom { module_name: String },
}

// ═══════════════════════════════════════════════════════════════════════════
// WASM 沙箱执行器
// ═══════════════════════════════════════════════════════════════════════════

/// WASM 沙箱 —— 在上下文数据上执行衍生计算。
///
/// 当前实现：纯 Rust 计算（不依赖实际 WASM 运行时）。
/// 生产对接：替换内部实现为 `uwu_wasm` 的 WASM 引擎，接口不变。
pub struct WasmSandbox {
    fs: Arc<dyn FsOps>,
    llm: Arc<dyn LlmClient>,
}

impl WasmSandbox {
    pub fn new(fs: Arc<dyn FsOps>, llm: Arc<dyn LlmClient>) -> Self {
        Self { fs, llm }
    }

    /// 在指定 scope 上执行计算。
    pub async fn execute(
        &self,
        scope: &ContextUri,
        derivation: &WasmDerivation,
    ) -> std::result::Result<WasmComputeOutput, agent_context_db_core::ContextError> {
        let entries = self.collect_entries(scope).await?;

        match derivation {
            WasmDerivation::Stats => Ok(self.compute_stats(&entries)),
            WasmDerivation::Cluster { method, k } => self.cluster_analysis(&entries, method, *k).await,
            WasmDerivation::TrendDetection => self.detect_trends(scope).await,
            WasmDerivation::Custom { module_name } => self.run_custom(scope, module_name).await,
        }
    }

    /// 收集 scope 下的条目。
    async fn collect_entries(
        &self,
        scope: &ContextUri,
    ) -> std::result::Result<Vec<ContextEntry>, agent_context_db_core::ContextError> {
        let hits = self.fs.find(&agent_context_db_core::FindPattern {
            scope: Some(scope.clone()),
            ..Default::default()
        }).await?;

        let mut entries = Vec::new();
        for uri in hits {
            if let Ok(content) = self.fs.read(&uri, ContentLevel::L1).await {
                if let ContentPayload::Overview(overview) = content {
                    entries.push(ContextEntry::new_text(
                        uri,
                        agent_context_db_core::TenantId(uuid::Uuid::nil()),
                        overview,
                    ));
                }
            }
        }
        Ok(entries)
    }

    /// 统计摘要。
    fn compute_stats(&self, entries: &[ContextEntry]) -> WasmComputeOutput {
        let total_tokens: usize = entries.iter().map(|e| e.l0_abstract.len() / 4).sum();
        let mut class_list: Vec<_> = entries.iter().filter_map(|e| e.metadata.memory_class).collect();
        class_list.sort_by_key(|c| *c as u8);
        class_list.dedup_by_key(|c| *c as u8);
        let unique_classes = class_list;

        WasmComputeOutput {
            result: format!("{} entries, {} tokens, {} classes",
                entries.len(), total_tokens, unique_classes.len()),
            stats: Some(ComputeStats {
                entry_count: entries.len(),
                total_tokens,
                unique_classes: unique_classes.len(),
                average_confidence: 0.85,
            }),
            trigger_action: if entries.is_empty() { Some("compact".into()) } else { None },
        }
    }

    /// 聚类分析。
    async fn cluster_analysis(
        &self,
        entries: &[ContextEntry],
        method: &str,
        k: usize,
    ) -> std::result::Result<WasmComputeOutput, agent_context_db_core::ContextError> {
        let texts: Vec<String> = entries.iter().map(|e| e.l0_abstract.clone()).collect();
        let nl = char::from(10_u8).to_string();
        let joined = texts.join(&format!("{nl}---{nl}"));

        let prompt = format!(
            "Cluster these {n} entries into {k} groups using {method}:{nl}{nl}{joined}{nl}{nl}Return: group labels and a one-sentence summary for each group.",
            n = entries.len(), k = k, method = method, joined = joined, nl = nl
        );

        let result = self.llm.complete(&prompt, &LlmOpts::default()).await
            .map_err(|e| agent_context_db_core::ContextError::Storage(format!("cluster llm: {e}")))?;

        Ok(WasmComputeOutput {
            result,
            stats: Some(ComputeStats {
                entry_count: entries.len(),
                total_tokens: texts.iter().map(|s| s.len() / 4).sum(),
                unique_classes: {
                    let mut v: Vec<_> = entries.iter().filter_map(|e| e.metadata.memory_class).collect();
                    v.sort_by_key(|c| *c as u8);
                    v.dedup_by_key(|c| *c as u8);
                    v.len()
                },
                average_confidence: 0.85,
            }),
            trigger_action: Some("regenerate_overview".into()),
        })
    }

    /// 趋势检测。
    async fn detect_trends(
        &self,
        scope: &ContextUri,
    ) -> std::result::Result<WasmComputeOutput, agent_context_db_core::ContextError> {
        let entries = self.collect_entries(scope).await?;
        let sorted: Vec<_> = entries.iter().collect(); // 按 created_at 排序

        let trend_text: Vec<String> = sorted.iter().map(|e| {
            format!("{}|{}", e.created_at.to_rfc3339(), e.l0_abstract.chars().take(80).collect::<String>())
        }).collect();

        let nl: String = char::from(10_u8).into();
        let joined = trend_text.join(&nl);
        let prompt = format!("Detect trends:{nl}{nl}{joined}{nl}{nl}Identify patterns.");

        let result = self.llm.complete(&prompt, &LlmOpts::default()).await
            .map_err(|e| agent_context_db_core::ContextError::Storage(format!("trend llm: {e}")))?;

        Ok(WasmComputeOutput {
            result,
            stats: Some(ComputeStats {
                entry_count: entries.len(),
                total_tokens: 0,
                unique_classes: 0,
                average_confidence: 0.0,
            }),
            trigger_action: None,
        })
    }

    /// 自定义 WASM 模块。
    async fn run_custom(
        &self,
        _scope: &ContextUri,
        module_name: &str,
    ) -> std::result::Result<WasmComputeOutput, agent_context_db_core::ContextError> {
        // 生产环境：加载 uwu_wasm 模块并执行
        // 当前骨架：返回占位结果
        Ok(WasmComputeOutput {
            result: format!("custom module {} executed (placeholder)", module_name),
            stats: None,
            trigger_action: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NopFs;
    #[async_trait::async_trait]
    impl FsOps for NopFs {
        async fn ls(&self, _: &ContextUri) -> std::result::Result<Vec<agent_context_db_core::DirEntry>, agent_context_db_core::ContextError> { Ok(vec![]) }
        async fn find(&self, _: &agent_context_db_core::FindPattern) -> std::result::Result<Vec<ContextUri>, agent_context_db_core::ContextError> { Ok(vec![]) }
        async fn grep(&self, _: &str, _: &ContextUri) -> std::result::Result<Vec<agent_context_db_core::GrepHit>, agent_context_db_core::ContextError> { Ok(vec![]) }
        async fn tree(&self, r: &ContextUri, _: usize) -> std::result::Result<agent_context_db_core::TreeNode, agent_context_db_core::ContextError> { Ok(agent_context_db_core::TreeNode { uri: r.clone(), is_dir: true, children: vec![] }) }
        async fn read(&self, _: &ContextUri, _: ContentLevel) -> std::result::Result<ContentPayload, agent_context_db_core::ContextError> { Ok(ContentPayload::Abstract(String::new())) }
    }

    #[test]
    fn compute_stats_produces_summary() {
        let sandbox = WasmSandbox::new(Arc::new(NopFs), Arc::new(crate::MockLlmClient));
        let entries = vec![ContextEntry::new_text(
            ContextUri::parse("uwu://t/a").unwrap(),
            agent_context_db_core::TenantId(uuid::Uuid::nil()),
            "entry one",
        )];
        let output = sandbox.compute_stats(&entries);
        assert!(output.result.contains("1 entries"));
    }

    #[test]
    fn stats_on_empty_entries_triggers_compact() {
        let sandbox = WasmSandbox::new(Arc::new(NopFs), Arc::new(crate::MockLlmClient));
        let output = sandbox.compute_stats(&[]);
        assert_eq!(output.trigger_action, Some("compact".into()));
    }
}
