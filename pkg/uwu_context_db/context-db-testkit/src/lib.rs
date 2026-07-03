//! # agent-context-db-testkit
//!
//! `FsOps` / `ContentRepo` / `VersionOps` / `TenantOps` 四个窄端口的内存参考实现，
//! 仅用于测试/开发。生产环境由 `agent-context-db` 注入 PG + Qdrant 后端。
//!
//! 用于 M1 检索层单测（无需启 PG）。

use agent_context_db_core::{
    ContentLevel, ContentPayload, ContentRepo, ContextDiff, ContextEntry, ContextError, ContextUri,
    DirEntry, FindPattern, FsOps, GrepHit, MvccVersion, Result, TenantId, TenantOps, TreeNode,
    VersionEntry, VersionOps,
};
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;

/// 内存版存储 —— 同时实现四个窄端口，故自动满足 `ContextStore`。
#[derive(Default)]
pub struct MemoryContextStore {
    // uri -> 版本序列（末尾为最新）
    entries: Mutex<HashMap<String, Vec<ContextEntry>>>,
}

impl MemoryContextStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn latest(&self, uri: &str) -> Option<ContextEntry> {
        self.entries.lock().get(uri).and_then(|v| v.last().cloned())
    }
}

#[async_trait]
impl ContentRepo for MemoryContextStore {
    async fn write(&self, mut entry: ContextEntry) -> Result<MvccVersion> {
        let mut map = self.entries.lock();
        let list = map.entry(entry.uri.0.clone()).or_default();
        let next = MvccVersion(list.len() as u64 + 1);
        entry.mvcc_version = next;
        entry.updated_at = chrono::Utc::now();
        list.push(entry);
        Ok(next)
    }

    async fn delete(&self, uri: &ContextUri) -> Result<()> {
        self.entries.lock().remove(&uri.0);
        Ok(())
    }

    async fn rename(&self, from: &ContextUri, to: &ContextUri) -> Result<()> {
        let mut map = self.entries.lock();
        let val = map
            .remove(&from.0)
            .ok_or_else(|| ContextError::NotFound(from.0.clone()))?;
        map.insert(to.0.clone(), val);
        Ok(())
    }
}

#[async_trait]
impl FsOps for MemoryContextStore {
    async fn ls(&self, dir: &ContextUri) -> Result<Vec<DirEntry>> {
        let prefix = format!("{}/", dir.0.trim_end_matches('/'));
        let map = self.entries.lock();
        let mut out = Vec::new();
        for (uri, versions) in map.iter() {
            if let Some(rest) = uri.strip_prefix(&prefix) {
                // 直接子项（rest 不含 `/`）视为文件，否则为目录
                let is_dir = rest.contains('/');
                let latest = versions.last().unwrap();
                out.push(DirEntry {
                    uri: ContextUri(uri.clone()),
                    is_dir,
                    abstract_: latest.l0_abstract.clone(),
                });
            }
        }
        Ok(out)
    }

    async fn find(&self, pattern: &FindPattern) -> Result<Vec<ContextUri>> {
        let map = self.entries.lock();
        let scope = pattern
            .scope
            .as_ref()
            .map(|u| u.0.clone())
            .unwrap_or_default();
        Ok(map
            .iter()
            .filter(|(uri, _)| uri.starts_with(&scope))
            .filter(|(_, versions)| match pattern.class {
                Some(c) => versions
                    .last()
                    .and_then(|e| e.metadata.memory_class)
                    .map(|mc| mc == c)
                    .unwrap_or(false),
                None => true,
            })
            .map(|(uri, _)| ContextUri(uri.clone()))
            .collect())
    }

    async fn grep(&self, regex: &str, scope: &ContextUri) -> Result<Vec<GrepHit>> {
        let needle = regex.to_lowercase();
        let map = self.entries.lock();
        let mut hits = Vec::new();
        for (uri, versions) in map.iter() {
            if !uri.starts_with(&scope.0) {
                continue;
            }
            if let Some(e) = versions.last() {
                if e.l0_abstract.to_lowercase().contains(&needle) {
                    hits.push(GrepHit {
                        uri: ContextUri(uri.clone()),
                        line: e.l0_abstract.clone(),
                        level: ContentLevel::L0,
                    });
                }
            }
        }
        Ok(hits)
    }

    async fn tree(&self, root: &ContextUri, _depth: usize) -> Result<TreeNode> {
        Ok(TreeNode {
            uri: root.clone(),
            is_dir: true,
            children: vec![],
        })
    }

    async fn read(&self, uri: &ContextUri, level: ContentLevel) -> Result<ContentPayload> {
        let e = self
            .latest(&uri.0)
            .ok_or_else(|| ContextError::NotFound(uri.0.clone()))?;
        Ok(match level {
            ContentLevel::L0 => ContentPayload::Abstract(e.l0_abstract),
            ContentLevel::L1 => ContentPayload::Overview(e.l1_overview.unwrap_or_default()),
            ContentLevel::L2 => ContentPayload::Detail(Vec::new()),
        })
    }
}

#[async_trait]
impl VersionOps for MemoryContextStore {
    async fn version_history(&self, uri: &ContextUri) -> Result<Vec<VersionEntry>> {
        Ok(self
            .entries
            .lock()
            .get(&uri.0)
            .map(|list| {
                list.iter()
                    .map(|e| VersionEntry {
                        version: e.mvcc_version,
                        message: e.l0_abstract.clone(),
                        ts: e.updated_at,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn rollback(&self, uri: &ContextUri, to: MvccVersion) -> Result<()> {
        let mut map = self.entries.lock();
        let list = map
            .get_mut(&uri.0)
            .ok_or_else(|| ContextError::NotFound(uri.0.clone()))?;
        let target = list
            .iter()
            .find(|e| e.mvcc_version == to)
            .cloned()
            .ok_or_else(|| ContextError::VersionConflict(format!("no version {:?}", to)))?;
        // rollback = 以旧版内容追加新版
        let mut restored = target;
        restored.mvcc_version = MvccVersion(list.len() as u64 + 1);
        list.push(restored);
        Ok(())
    }

    async fn diff(&self, uri: &ContextUri, a: MvccVersion, b: MvccVersion) -> Result<ContextDiff> {
        Ok(ContextDiff {
            summary: format!("{}: v{:?} → v{:?}", uri.0, a, b),
        })
    }
}

#[async_trait]
impl TenantOps for MemoryContextStore {
    async fn list_tenants(&self) -> Result<Vec<TenantId>> {
        let map = self.entries.lock();
        let mut set: Vec<TenantId> = map
            .values()
            .filter_map(|v| v.last().map(|e| e.tenant))
            .collect();
        set.sort_by_key(|t| t.0);
        set.dedup_by_key(|t| t.0);
        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_context_db_core::ContextStore;
    use uuid::Uuid;

    fn entry(uri: &str, text: &str) -> ContextEntry {
        ContextEntry::new_text(
            ContextUri::parse(uri).unwrap(),
            TenantId(Uuid::nil()),
            text,
        )
    }

    #[tokio::test]
    async fn write_read_ls_roundtrip() {
        let store = MemoryContextStore::new();
        store
            .write(entry("uwu://t/agent/a/memories/cases/c1", "solved bug X"))
            .await
            .unwrap();

        // read L0
        let p = store
            .read(
                &ContextUri::parse("uwu://t/agent/a/memories/cases/c1").unwrap(),
                ContentLevel::L0,
            )
            .await
            .unwrap();
        assert!(matches!(p, ContentPayload::Abstract(s) if s == "solved bug X"));

        // ls parent dir
        let dir = ContextUri::parse("uwu://t/agent/a/memories/cases").unwrap();
        assert_eq!(store.ls(&dir).await.unwrap().len(), 1);

        // grep
        let hits = store
            .grep("bug", &ContextUri::parse("uwu://t").unwrap())
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn version_history_and_rollback() {
        let store = MemoryContextStore::new();
        let uri = ContextUri::parse("uwu://t/agent/a/state/mid/s1").unwrap();
        let v1 = store.write(entry(&uri.0, "v1")).await.unwrap();
        store.write(entry(&uri.0, "v2")).await.unwrap();
        assert_eq!(store.version_history(&uri).await.unwrap().len(), 2);

        store.rollback(&uri, v1).await.unwrap();
        assert_eq!(store.version_history(&uri).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn context_store_supertrait_is_satisfied() {
        // 编译期验证：MemoryContextStore 自动实现聚合 ContextStore。
        fn assert_store<T: ContextStore>() {}
        assert_store::<MemoryContextStore>();
    }
}
