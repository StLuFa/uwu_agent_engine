//! 基于 [Loro](https://loro.dev) 的可移动树 CRDT 封装。
//!
//! [`UwuCrdtDoc`] 面向「文档 / 图」这类**层级结构**，提供领域无关的树操作与
//! [`UwuOp`] 应用接口。调用方（`uwu_wiki::wiki-collab`、`agent-context-db`）把各自
//! 的领域操作翻译成 [`UwuOp`]，本模块负责无冲突合并 + 快照/增量导入导出。
//!
//! # 关键映射
//!
//! Loro 内部用自己生成的 `TreeID` 标识节点，而 wiki/context-db 用稳定的外部字符串
//! id（如 `BlockId`）。[`UwuCrdtDoc`] 在节点 meta 中写入 `uid` 字段，并维护
//! `NodeId → TreeID` 缓存；`import` 后通过遍历树重建该缓存，保证跨副本一致。
//!
//! # 存储职责
//!
//! 本类型**不持久化**任何东西：`export_snapshot` / `export_updates` 产出字节由调用方
//! 落库（`WikiStorage::doc_store` / `op_log`）。DB 是唯一真相源。

use loro::{ExportMode, LoroDoc, LoroMap, LoroText, LoroTree, TreeID, TreeParentId, VersionVector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// 外部稳定节点标识（如 wiki `BlockId`、graph `NodeId`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl<S: Into<String>> From<S> for NodeId {
    fn from(s: S) -> Self {
        NodeId(s.into())
    }
}

/// 封装层错误。
#[derive(Debug, Error)]
pub enum UwuCrdtError {
    #[error("loro: {0}")]
    Loro(String),
    #[error("node not found: {0}")]
    NodeNotFound(String),
    #[error("serialization: {0}")]
    Serialization(String),
}

type Result<T> = std::result::Result<T, UwuCrdtError>;

impl From<loro::LoroError> for UwuCrdtError {
    fn from(e: loro::LoroError) -> Self {
        UwuCrdtError::Loro(e.to_string())
    }
}

impl From<loro::LoroEncodeError> for UwuCrdtError {
    fn from(e: loro::LoroEncodeError) -> Self {
        UwuCrdtError::Loro(e.to_string())
    }
}

/// 领域无关的树操作 —— CRDT 合并输入。
///
/// wiki 的 `Op::InsertBlock/UpdateBlock/DeleteBlock/MoveBlock` 一一翻译到此枚举。
///
/// # 子容器：`Text*` 与 `Map*`
///
/// 需要**真正的**并发合并（不是覆盖）时，使用挂在节点 meta 上的 Loro 子容器：
///
/// - `TextSplice`：单元格文本、Block 富文本 —— CRDT 文本合并（Fugue 算法）。
/// - `MapSet` / `MapDelete`：图边集合、表格单元格字段 —— LWW-Map，键级合并，
///   并发写不同键无冲突，写同键取更新的一方。
///
/// `slot` 是 meta 上的子容器键名，同一节点可挂多个（如 `"title"`/`"body"`/`"edges"`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UwuOp {
    /// 插入节点。`parent = None` 表示根节点；`after` 指定同级插入位置（其后）。
    Insert {
        id: NodeId,
        parent: Option<NodeId>,
        after: Option<NodeId>,
        /// 领域负载（block 内容 / 节点标签等），存于节点 meta。
        data: Value,
    },
    /// 合并式更新节点负载（浅合并顶层字段，LWW；同键并发写后写覆盖）。
    Update { id: NodeId, patch: Value },
    /// 删除节点（其子树在状态中不再可见）。
    Delete { id: NodeId },
    /// 移动节点到新父 / 新位置。
    Move {
        id: NodeId,
        new_parent: Option<NodeId>,
        after: Option<NodeId>,
    },

    // ---- 子容器：真正的 CRDT 合并 ----
    /// 对 `id.meta[slot]` 挂载的 LoroText 做 splice（unicode 位置）。
    /// 首次访问时按需创建（mergeable）。
    TextSplice {
        id: NodeId,
        slot: String,
        /// unicode 起点。
        pos: usize,
        /// 删除的 unicode 长度。
        del: usize,
        /// 要插入的文本。
        insert: String,
    },
    /// 对 `id.meta[slot]` 挂载的 LoroMap 写入 `key → value`（LWW）。
    /// 用途：图边集合（key = edge_id）、单元格集合（key = column_id）。
    MapSet {
        id: NodeId,
        slot: String,
        key: String,
        value: Value,
    },
    /// 从 `id.meta[slot]` LoroMap 删除某 key。
    MapDelete { id: NodeId, slot: String, key: String },
}

/// 节点 meta 中存放外部 id 的键。
const UID_KEY: &str = "uid";
/// 节点 meta 中存放领域负载 JSON 的键。
const DATA_KEY: &str = "data";
/// 树容器名。
const TREE_NAME: &str = "nodes";

/// 可移动树 CRDT 文档。
pub struct UwuCrdtDoc {
    doc: LoroDoc,
    node_map: HashMap<NodeId, TreeID>,
}

impl UwuCrdtDoc {
    /// 新建空文档。`peer_id` 用于区分并发副本（同一逻辑文档的不同 Agent/客户端）。
    pub fn new(peer_id: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer_id).ok();
        // 开启分数索引，支持稳定的同级排序与并发插入。
        doc.get_tree(TREE_NAME).enable_fractional_index(0);
        Self { doc, node_map: HashMap::new() }
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree(TREE_NAME)
    }

    fn tree_id(&self, id: &NodeId) -> Result<TreeID> {
        self.node_map
            .get(id)
            .copied()
            .ok_or_else(|| UwuCrdtError::NodeNotFound(id.0.clone()))
    }

    fn parent_of(&self, parent: &Option<NodeId>) -> Result<TreeParentId> {
        match parent {
            None => Ok(TreeParentId::Root),
            Some(p) => Ok(TreeParentId::Node(self.tree_id(p)?)),
        }
    }

    fn write_meta(&self, tid: TreeID, uid: &str, data: &Value) -> Result<()> {
        let meta = self.tree().get_meta(tid)?;
        meta.insert(UID_KEY, uid)?;
        meta.insert(
            DATA_KEY,
            serde_json::to_string(data).map_err(|e| UwuCrdtError::Serialization(e.to_string()))?,
        )?;
        Ok(())
    }

    /// 应用单个操作（不自动 commit，便于批量）。
    pub fn apply(&mut self, op: &UwuOp) -> Result<()> {
        match op {
            UwuOp::Insert { id, parent, after, data } => {
                let parent_pid = self.parent_of(parent)?;
                let tid = self.tree().create(parent_pid)?;
                if let Some(a) = after {
                    let after_tid = self.tree_id(a)?;
                    self.tree().mov_after(tid, after_tid)?;
                }
                self.write_meta(tid, &id.0, data)?;
                self.node_map.insert(id.clone(), tid);
            }
            UwuOp::Update { id, patch } => {
                let tid = self.tree_id(id)?;
                let mut cur = self.read_data(tid)?;
                merge_patch(&mut cur, patch);
                self.write_meta(tid, &id.0, &cur)?;
            }
            UwuOp::Delete { id } => {
                let tid = self.tree_id(id)?;
                self.tree().delete(tid)?;
                self.node_map.remove(id);
            }
            UwuOp::Move { id, new_parent, after } => {
                let tid = self.tree_id(id)?;
                let parent_pid = self.parent_of(new_parent)?;
                self.tree().mov(tid, parent_pid)?;
                if let Some(a) = after {
                    let after_tid = self.tree_id(a)?;
                    self.tree().mov_after(tid, after_tid)?;
                }
            }
            UwuOp::TextSplice { id, slot, pos, del, insert } => {
                let tid = self.tree_id(id)?;
                let text = self.ensure_text(tid, slot)?;
                text.splice(*pos, *del, insert)?;
            }
            UwuOp::MapSet { id, slot, key, value } => {
                let tid = self.tree_id(id)?;
                let map = self.ensure_map(tid, slot)?;
                // 存 JSON 字符串，避免 LoroValue 与 serde_json::Value 的类型墙。
                let s = serde_json::to_string(value)
                    .map_err(|e| UwuCrdtError::Serialization(e.to_string()))?;
                map.insert(key, s)?;
            }
            UwuOp::MapDelete { id, slot, key } => {
                let tid = self.tree_id(id)?;
                let map = self.ensure_map(tid, slot)?;
                map.delete(key)?;
            }
        }
        Ok(())
    }

    /// 拿到（或创建）挂在节点 meta 上的 LoroText 子容器。mergeable：跨副本同 slot 收敛。
    fn ensure_text(&self, tid: TreeID, slot: &str) -> Result<LoroText> {
        let meta = self.tree().get_meta(tid)?;
        Ok(meta.ensure_mergeable_text(slot)?)
    }

    /// 拿到（或创建）挂在节点 meta 上的 LoroMap 子容器。mergeable：跨副本同 slot 收敛。
    fn ensure_map(&self, tid: TreeID, slot: &str) -> Result<LoroMap> {
        let meta = self.tree().get_meta(tid)?;
        Ok(meta.ensure_mergeable_map(slot)?)
    }

    /// 读取节点某 slot 的 LoroText 内容（若不存在返回空串）。
    pub fn read_text(&self, id: &NodeId, slot: &str) -> Result<String> {
        let tid = self.tree_id(id)?;
        let meta = self.tree().get_meta(tid)?;
        let text = meta.ensure_mergeable_text(slot)?;
        Ok(text.to_string())
    }

    /// 读取节点某 slot 的 LoroMap 全量（key → JSON）。
    pub fn read_map(&self, id: &NodeId, slot: &str) -> Result<std::collections::BTreeMap<String, Value>> {
        let tid = self.tree_id(id)?;
        let meta = self.tree().get_meta(tid)?;
        let map = meta.ensure_mergeable_map(slot)?;
        let mut out = std::collections::BTreeMap::new();
        map.for_each(|k, voc| {
            if let Some(v) = voc.into_value().ok().and_then(|v| v.into_string().ok())
                && let Ok(json) = serde_json::from_str::<Value>(v.as_str())
            {
                out.insert(k.to_string(), json);
            }
        });
        Ok(out)
    }

    /// 批量应用并 commit。
    pub fn apply_ops(&mut self, ops: &[UwuOp]) -> Result<()> {
        for op in ops {
            self.apply(op)?;
        }
        self.doc.commit();
        Ok(())
    }

    /// 读取节点当前负载。
    pub fn read_data(&self, tid: TreeID) -> Result<Value> {
        let meta = self.tree().get_meta(tid)?;
        match meta.get(DATA_KEY) {
            Some(voc) => {
                let s = voc
                    .into_value()
                    .ok()
                    .and_then(|v| v.into_string().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if s.is_empty() {
                    Ok(Value::Null)
                } else {
                    serde_json::from_str(&s)
                        .map_err(|e| UwuCrdtError::Serialization(e.to_string()))
                }
            }
            None => Ok(Value::Null),
        }
    }

    /// 按外部 id 读取负载。
    pub fn get(&self, id: &NodeId) -> Result<Value> {
        let tid = self.tree_id(id)?;
        self.read_data(tid)
    }

    /// 当前存活节点数。
    pub fn len(&self) -> usize {
        self.node_map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.node_map.is_empty()
    }

    // ---------------------------------------------------------------------
    // 合并 / 导入导出
    // ---------------------------------------------------------------------

    /// 合并另一副本导出的字节（快照或增量均可）。合并后重建 id 映射。
    pub fn import(&mut self, bytes: &[u8]) -> Result<()> {
        self.doc.import(bytes)?;
        self.rebuild_map();
        Ok(())
    }

    /// 全量快照（含历史），用于新副本首次加载 / 落库 doc_store。
    pub fn export_snapshot(&self) -> Result<Vec<u8>> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

    /// 自 `since`（对端版本向量编码）以来的增量 Op，用于写 op_log / 广播。
    /// `since = None` 导出全部历史。
    pub fn export_updates(&self, since: Option<&[u8]>) -> Result<Vec<u8>> {
        let vv = match since {
            Some(bytes) => VersionVector::decode(bytes)?,
            None => VersionVector::default(),
        };
        Ok(self.doc.export(ExportMode::updates(&vv))?)
    }

    /// 当前 oplog 版本向量（编码后），供对端下次增量导出用。
    pub fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    /// 遍历树，用节点 meta 里的 `uid` 重建 `NodeId → TreeID` 映射。
    fn rebuild_map(&mut self) {
        let tree = self.tree();
        let mut map = HashMap::new();
        for tid in tree.nodes() {
            if tree.is_node_deleted(&tid).unwrap_or(true) {
                continue;
            }
            if let Ok(meta) = tree.get_meta(tid)
                && let Some(uid) = meta
                    .get(UID_KEY)
                    .and_then(|v| v.into_value().ok())
                    .and_then(|v| v.into_string().ok())
                    .map(|s| s.to_string())
            {
                map.insert(NodeId(uid), tid);
            }
        }
        self.node_map = map;
    }
}

/// 顶层字段浅合并：把 `patch` 的键覆盖进 `target`（两者均为 JSON 对象时）。
fn merge_patch(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                t.insert(k.clone(), v.clone());
            }
        }
        (t, p) => *t = p.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn insert(id: &str, parent: Option<&str>, text: &str) -> UwuOp {
        UwuOp::Insert {
            id: id.into(),
            parent: parent.map(|p| p.into()),
            after: None,
            data: json!({ "text": text }),
        }
    }

    #[test]
    fn insert_and_read() {
        let mut doc = UwuCrdtDoc::new(1);
        doc.apply_ops(&[insert("root", None, "hello")]).unwrap();
        assert_eq!(doc.get(&"root".into()).unwrap(), json!({ "text": "hello" }));
        assert_eq!(doc.len(), 1);
    }

    #[test]
    fn update_merges_patch() {
        let mut doc = UwuCrdtDoc::new(1);
        doc.apply_ops(&[insert("a", None, "hi")]).unwrap();
        doc.apply_ops(&[UwuOp::Update {
            id: "a".into(),
            patch: json!({ "bold": true }),
        }])
        .unwrap();
        assert_eq!(doc.get(&"a".into()).unwrap(), json!({ "text": "hi", "bold": true }));
    }

    #[test]
    fn delete_removes_node() {
        let mut doc = UwuCrdtDoc::new(1);
        doc.apply_ops(&[insert("a", None, "x")]).unwrap();
        doc.apply_ops(&[UwuOp::Delete { id: "a".into() }]).unwrap();
        assert!(doc.is_empty());
        assert!(doc.get(&"a".into()).is_err());
    }

    #[test]
    fn move_reparents_node() {
        let mut doc = UwuCrdtDoc::new(1);
        doc.apply_ops(&[
            insert("p1", None, "parent1"),
            insert("p2", None, "parent2"),
            insert("c", Some("p1"), "child"),
        ])
        .unwrap();
        doc.apply_ops(&[UwuOp::Move {
            id: "c".into(),
            new_parent: Some("p2".into()),
            after: None,
        }])
        .unwrap();
        // 移动后子节点仍存活、负载不变。
        assert_eq!(doc.get(&"c".into()).unwrap(), json!({ "text": "child" }));
    }

    #[test]
    fn snapshot_roundtrip_rebuilds_map() {
        let mut a = UwuCrdtDoc::new(1);
        a.apply_ops(&[insert("root", None, "r"), insert("child", Some("root"), "c")])
            .unwrap();
        let snap = a.export_snapshot().unwrap();

        let mut b = UwuCrdtDoc::new(2);
        b.import(&snap).unwrap();
        // 导入后可按外部 id 访问，说明映射已重建。
        assert_eq!(b.get(&"root".into()).unwrap(), json!({ "text": "r" }));
        assert_eq!(b.get(&"child".into()).unwrap(), json!({ "text": "c" }));
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn concurrent_merge_no_conflict() {
        let mut a = UwuCrdtDoc::new(1);
        a.apply_ops(&[insert("root", None, "r")]).unwrap();
        let snap = a.export_snapshot().unwrap();

        let mut b = UwuCrdtDoc::new(2);
        b.import(&snap).unwrap();

        // 两副本各自并发插入不同子节点。
        a.apply_ops(&[insert("a1", Some("root"), "from-a")]).unwrap();
        b.apply_ops(&[insert("b1", Some("root"), "from-b")]).unwrap();

        // 交换增量。
        let a_updates = a.export_updates(None).unwrap();
        let b_updates = b.export_updates(None).unwrap();
        a.import(&b_updates).unwrap();
        b.import(&a_updates).unwrap();

        // 收敛：两副本都应有 root + a1 + b1。
        assert_eq!(a.len(), 3);
        assert_eq!(b.len(), 3);
        assert_eq!(a.get(&"b1".into()).unwrap(), json!({ "text": "from-b" }));
        assert_eq!(b.get(&"a1".into()).unwrap(), json!({ "text": "from-a" }));
    }

    #[test]
    fn incremental_export_since_version() {
        let mut a = UwuCrdtDoc::new(1);
        a.apply_ops(&[insert("root", None, "r")]).unwrap();
        let v1 = a.version();

        a.apply_ops(&[insert("c", Some("root"), "c")]).unwrap();
        let delta = a.export_updates(Some(&v1)).unwrap();

        let mut b = UwuCrdtDoc::new(2);
        b.import(&a.export_updates(None).unwrap()).unwrap();
        // delta 仅含第二次插入，导入后仍收敛。
        b.import(&delta).unwrap();
        assert_eq!(b.len(), 2);
    }

    // -------- 子容器：TextSplice / MapSet / MapDelete --------

    #[test]
    fn text_splice_local_edits() {
        let mut doc = UwuCrdtDoc::new(1);
        doc.apply_ops(&[insert("cell", None, "")]).unwrap();
        doc.apply_ops(&[
            UwuOp::TextSplice {
                id: "cell".into(),
                slot: "body".into(),
                pos: 0,
                del: 0,
                insert: "Hello world".into(),
            },
            UwuOp::TextSplice {
                id: "cell".into(),
                slot: "body".into(),
                pos: 5,
                del: 0,
                insert: ",".into(),
            },
        ])
        .unwrap();
        assert_eq!(doc.read_text(&"cell".into(), "body").unwrap(), "Hello, world");
    }

    #[test]
    fn text_splice_concurrent_edits_converge() {
        // 表格单元格的并发富文本编辑 —— CRDT 文本合并。
        let mut a = UwuCrdtDoc::new(1);
        a.apply_ops(&[insert("cell", None, "")]).unwrap();
        a.apply_ops(&[UwuOp::TextSplice {
            id: "cell".into(),
            slot: "body".into(),
            pos: 0,
            del: 0,
            insert: "abc".into(),
        }])
        .unwrap();
        let snap = a.export_snapshot().unwrap();

        let mut b = UwuCrdtDoc::new(2);
        b.import(&snap).unwrap();

        // a 在头部插入 "X"，b 在尾部插入 "Y"。
        a.apply_ops(&[UwuOp::TextSplice {
            id: "cell".into(),
            slot: "body".into(),
            pos: 0,
            del: 0,
            insert: "X".into(),
        }])
        .unwrap();
        b.apply_ops(&[UwuOp::TextSplice {
            id: "cell".into(),
            slot: "body".into(),
            pos: 3,
            del: 0,
            insert: "Y".into(),
        }])
        .unwrap();

        a.import(&b.export_updates(None).unwrap()).unwrap();
        b.import(&a.export_updates(None).unwrap()).unwrap();

        // 两副本收敛且都包含双方编辑。
        let ta = a.read_text(&"cell".into(), "body").unwrap();
        let tb = b.read_text(&"cell".into(), "body").unwrap();
        assert_eq!(ta, tb);
        assert!(ta.contains('X') && ta.contains('Y') && ta.contains("abc"));
    }

    #[test]
    fn map_set_and_delete() {
        // 图边集合：edges slot 里 key=edge_id, value=边负载。
        let mut doc = UwuCrdtDoc::new(1);
        doc.apply_ops(&[insert("graph", None, "")]).unwrap();
        doc.apply_ops(&[
            UwuOp::MapSet {
                id: "graph".into(),
                slot: "edges".into(),
                key: "e1".into(),
                value: json!({ "from": "a", "to": "b" }),
            },
            UwuOp::MapSet {
                id: "graph".into(),
                slot: "edges".into(),
                key: "e2".into(),
                value: json!({ "from": "b", "to": "c" }),
            },
            UwuOp::MapDelete {
                id: "graph".into(),
                slot: "edges".into(),
                key: "e1".into(),
            },
        ])
        .unwrap();
        let m = doc.read_map(&"graph".into(), "edges").unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m["e2"], json!({ "from": "b", "to": "c" }));
    }

    #[test]
    fn map_concurrent_disjoint_keys_converge() {
        // 两个副本并发插入不同边，合并后都在。
        let mut a = UwuCrdtDoc::new(1);
        a.apply_ops(&[insert("g", None, "")]).unwrap();
        let snap = a.export_snapshot().unwrap();

        let mut b = UwuCrdtDoc::new(2);
        b.import(&snap).unwrap();

        a.apply_ops(&[UwuOp::MapSet {
            id: "g".into(),
            slot: "edges".into(),
            key: "e-a".into(),
            value: json!({ "from": "1", "to": "2" }),
        }])
        .unwrap();
        b.apply_ops(&[UwuOp::MapSet {
            id: "g".into(),
            slot: "edges".into(),
            key: "e-b".into(),
            value: json!({ "from": "3", "to": "4" }),
        }])
        .unwrap();

        a.import(&b.export_updates(None).unwrap()).unwrap();
        b.import(&a.export_updates(None).unwrap()).unwrap();

        let ma = a.read_map(&"g".into(), "edges").unwrap();
        let mb = b.read_map(&"g".into(), "edges").unwrap();
        assert_eq!(ma, mb);
        assert_eq!(ma.len(), 2);
    }
}
