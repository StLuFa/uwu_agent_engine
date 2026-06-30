# agent-wiki

Agent **协作知识库** —— 多 Agent 共享的结构化 Wiki，MVCC 版本化 + 可插拔存储后端。

## 概述

agent-wiki 是 LLM Wiki 的核心实现：一个被多个 Agent 协作编辑的结构化知识库。每个 WikiPage 携带完整版本历史，支持回滚、差异比较和语义搜索。

```
Wiki 操作流:
  创建:  Perception → WikiRepo.save(page)
  编辑:  fork(State) → 沙盒推演 → evaluate() → edit() → WikiRepo.save()
  搜索:  WikiRepo.search(query) → 全文/语义匹配
  版本:  version_history → diff_versions(v1, v2) → PageDiff
  回滚:  rollback_to(v) → 新版本（保留完整历史）
```

## 特性

- **MVCC 版本化** — 每次编辑追加 `WikiPageVersion` 到历史，`edit()` 递增 `current_version`
- **版本差异** — `diff_versions(v1, v2)` 比较标题/内容变化量
- **版本回滚** — `rollback_to(v)` 创建新版本（不删除历史）
- **可插拔存储** — `WikiRepo` trait 抽象，当前 `MemoryWikiStore`，后续接 PostgreSQL + 向量检索
- **全文搜索** — `search(query)` 按标题/内容/标签匹配，标题匹配优先
- **多维度筛选** — `by_tag()`, `by_category()`, `by_status()`, `list(offset, limit)`
- **页面引用图** — `references` / `referenced_by` 构建页面间链接
- **标题去重** — 同标题页面拒绝保存

## 安装

```toml
[dependencies]
agent-wiki = { path = "../agent-wiki" }
```

## 快速上手

### 创建和发布页面

```rust
use agent_wiki::{WikiPage, PageStatus, MemoryWikiStore, WikiRepo};

let mut store = MemoryWikiStore::new();

let mut page = WikiPage::new(
    "Rust Async Basics",
    "# Async\n\nRust async/await introduction",
    "rust",
    "agent-1",
);
page.add_tag("rust");
page.add_tag("async");
page.publish();

store.save(&page).await.unwrap();
```

### 编辑页面

```rust
let mut page = store.get(&page_id).await.unwrap().unwrap();
page.edit(
    "Rust Async Deep Dive",     // 新标题
    "# Async\n\n## Tokio\n...", // 新内容 (Markdown)
    "expanded with Tokio section",
    "agent-2",
);
store.save(&page).await.unwrap();

assert_eq!(page.current_version, 1);
assert_eq!(page.version_history.len(), 2);
```

### 版本历史与回滚

```rust
// 查看历史版本
let v0 = page.version_at(0).unwrap();
println!("v0: {} by {}", v0.edit_summary, v0.edited_by);

// 回滚到版本 0（创建新版本 2）
page.rollback_to(0, "agent-1");
assert_eq!(page.current_version, 2);

// 版本差异
let diff = page.diff_versions(0, 1).unwrap();
println!("added: {} chars, removed: {} chars", diff.content_added, diff.content_removed);
```

### 搜索和筛选

```rust
// 全文搜索
let results = store.search("async tokio").await.unwrap();

// 按标签
let rust_pages = store.by_tag("rust").await.unwrap();

// 按分类
let docs = store.by_category("rust").await.unwrap();

// 按状态
let drafts = store.by_status(PageStatus::Draft).await.unwrap();

// 分页列表（按更新时间降序）
let latest = store.list(0, 20).await.unwrap();
```

### 页面引用

```rust
let mut page_a = WikiPage::new("Page A", "refs B", "general", "agent-1");
page_a.add_reference(&page_b.page_id);
// 后续：构建引用图、反向链接等
```

### 实现自定义存储后端

```rust
use agent_wiki::{WikiRepo, WikiRepoError, WikiPage, PageStatus};
use async_trait::async_trait;

struct PostgresWikiStore { /* pg_pool: PgPool */ }

#[async_trait]
impl WikiRepo for PostgresWikiStore {
    async fn save(&mut self, page: &WikiPage) -> Result<(), WikiRepoError> {
        // UPSERT INTO wiki_pages ...
        Ok(())
    }
    async fn search(&self, query: &str) -> Result<Vec<WikiPage>, WikiRepoError> {
        // SELECT ... WHERE to_tsvector(content) @@ plainto_tsquery(query)
        Ok(vec![])
    }
    // ... 其余方法
}
```

## 核心类型

### WikiPage

```rust
pub struct WikiPage {
    pub page_id: String,
    pub title: String,
    pub content: String,              // Markdown
    pub tags: Vec<String>,
    pub category: String,
    pub status: PageStatus,           // Draft / Published / Archived
    pub current_version: u64,
    pub version_history: Vec<WikiPageVersion>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub references: Vec<String>,      // 引用的页面 ID
    pub referenced_by: Vec<String>,   // 被引用的页面 ID
}
```

方法：

| 方法 | 说明 |
|---|---|
| `new(title, content, category, agent)` | 创建草稿（版本 0） |
| `edit(title, content, summary, agent)` | 追加新版本，version += 1 |
| `rollback_to(version, agent)` | 回滚 → 新版本 |
| `publish()` / `archive()` | 状态切换 |
| `add_tag(tag)` / `add_reference(page_id)` | 元数据 |
| `version_at(v)` / `diff_versions(v1, v2)` | 版本查询 |

### WikiPageVersion

```rust
pub struct WikiPageVersion {
    pub version: u64,
    pub title: String,
    pub content: String,
    pub edit_summary: String,
    pub edited_by: String,
    pub edited_at: DateTime<Utc>,
}
```

### PageDiff

```rust
pub struct PageDiff {
    pub title_changed: bool,
    pub content_added: usize,
    pub content_removed: usize,
    pub v1: u64,
    pub v2: u64,
}
```

### WikiRepo trait

```rust
#[async_trait]
pub trait WikiRepo: Send + Sync {
    async fn save(&mut self, page: &WikiPage) -> Result<(), WikiRepoError>;
    async fn get(&self, page_id: &str) -> Result<Option<WikiPage>, WikiRepoError>;
    async fn get_by_title(&self, title: &str) -> Result<Option<WikiPage>, WikiRepoError>;
    async fn search(&self, query: &str) -> Result<Vec<WikiPage>, WikiRepoError>;
    async fn by_tag(&self, tag: &str) -> Result<Vec<WikiPage>, WikiRepoError>;
    async fn by_category(&self, category: &str) -> Result<Vec<WikiPage>, WikiRepoError>;
    async fn by_status(&self, status: PageStatus) -> Result<Vec<WikiPage>, WikiRepoError>;
    async fn delete(&mut self, page_id: &str) -> Result<bool, WikiRepoError>;
    async fn count(&self) -> Result<usize, WikiRepoError>;
    async fn list(&self, offset: usize, limit: usize) -> Result<Vec<WikiPage>, WikiRepoError>;
}
```

### PageStatus

```rust
pub enum PageStatus {
    Draft,       // 草稿
    Published,   // 已发布
    Archived,    // 已归档
}
```

## 架构集成

```
agent-wiki ── 读 ──▶ agent-state      (版本化模式参考)
           ── 接 ──▶ uwu_database     (生产存储: DatabaseWikiStore, VectorStore + PostgreSQL)
           ── 接 ──▶ agent-crdt       (多 Agent 无冲突编辑)
           ── 接 ──▶ agent-mesh       (wiki 变更事件: agent.wiki.updated)
           ── 接 ──▶ agent-collaboration (委派编辑任务)
           ── 接 ──▶ agent-guard      (编辑权限控制)
           ◀── 被 agent-session + agent-collaboration 消费
```

## 目录结构

```
src/
├── lib.rs      // re-exports + 集成测试
├── page.rs     // WikiPage + WikiPageVersion + PageDiff + PageStatus
├── repo.rs     // WikiRepo trait + WikiRepoError
└── store.rs    // MemoryWikiStore (开发用)
```

## 测试

```bash
cargo test -p agent-wiki
```

当前测试：12 个（启用 database feature 后 14 个）。覆盖：页面创建/编辑/发布/归档、版本递增、回滚创建新版本、版本差异、全文搜索、标签筛选、标题去重、删除、分页列表、多页面搜索。

## 依赖

- `agent-types-core` — AgentId
- `serde` + `serde_json` — 序列化
- `chrono` — 时间戳
- `uuid` — 页面 ID 生成
- `async-trait` + `tokio` — async trait + 运行时

## License

与仓库一致。
