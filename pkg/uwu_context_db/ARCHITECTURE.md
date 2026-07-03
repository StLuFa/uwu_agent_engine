# agent-context-db：类 OpenViking 上下文数据库完整架构设计

> 自主实现一个类 OpenViking 的上下文数据库，完全复刻 OpenViking 核心机制并与五维深度融合，允许对现有 crate 做破坏性变更。

本文档为完整统一的架构设计，涵盖基础架构、版本管理、创新功能、风险根治与性能优化。

---

## 0. 设计目标与定位

### 0.1 一句话定位

**agent-context-db 是通用的 Agent 上下文数据库核心（L1-L7）**，以文件系统范式统一管理 Memory / Resource / Skill / Wiki，吸收 OpenViking 全部核心机制（L0/L1/L2 渐进加载、8 种记忆分类、LLM 去重、轨迹/经验两层、两阶段 commit、目录递归检索、检索轨迹可视化）。

**agent-context-db-uwu 是 uwu_agent_engine 的专有扩展（L8）**，在通用核心基础上叠加五维深度耦合（State fork 推演、Metacog 校准、Guard 写入约束、Character 价值观映射）和 LLM Wiki 工作流，使上下文存储与决策层彻底同构。

**两者并行**：外部 Agent 项目使用通用核心，uwu_agent_engine 使用专有扩展（re-export 通用核心 + 叠加专有能力）。

### 0.2 三大设计原则

1. **FS 范式统一性**：一切上下文皆 URI（`uwu://`），可 `ls`/`find`/`grep`/`tree`/`read`，避免"向量黑盒"。
2. **双层存储单一数据源**：内容层（PostgreSQL 模拟 AGFS）= 唯一真相源；索引层（Qdrant）只存 URI + 向量 + 元数据指针，绝不存文件内容。
3. **通用核心与专有扩展分离**：L1-L7 与具体 Agent 框架无关，可独立发布；L8 强依赖 uwu crate，单独成包（agent-context-db-uwu）。
4. **事实层 / 派生层分离**（对接已落地 crate 的总原则）：凡是已在内存中运行的热态（`AgentState` 标量、`CalibrationHistory` 环形缓冲、内存关系图）是**派生层的真值源**；context-db 存的是它们的**冷归档 + 可重算来源**,不是反向真值。任何"把热态挪进库"的设计必须先证明热路径不会因此增加 IO。此原则优先于 FS 范式统一性——不为了"一切皆 URI"而牺牲高频路径性能。

### 0.3 与 OpenViking 的定位差异

| | OpenViking | agent-context-db (通用核心) | agent-context-db-uwu (专有扩展) |
|---|---|---|---|
| 角色 | 独立上下文 DB 产品 | 通用 Agent 上下文库，可独立发布 | uwu_agent_engine 内嵌，深度耦合五维 |
| Agent 视角 | Agent 的"外设" | Agent 的"结构化记忆层" | Agent 五维的"内部存储" |
| 决策耦合 | 解耦（只管存/找） | 解耦（通用检索 + MVCC） | 深度耦合（State fork、Metacog 校准、Guard 写入约束） |
| 安全模型 | 数据加密 + 多租户 | 多租户 + 基础权限 | GuardLayer 五层 + 数据加密 + 多租户 |
| 多 Agent | 单租户文件操作 | CRDT 合并（通用） | CRDT + agent-collaboration 协作编辑 |
| 语言 | Python + Rust 混合 | 全 Rust | 全 Rust |
| LLM 集成 | 内置多 Provider 配置 | `LlmClient` trait 抽象 | MCP 优先 + 直连 SDK 备选 |
| 部署模式 | 嵌入式 + HTTP 双模式 | 第一版仅嵌入式，HTTP 延后 | 同左 |

### 0.4 关键决策记录

| 决策项 | 选择 | 理由 |
|---|---|---|
| **通用核心与专有扩展分离** | **L1-L7 → agent-context-db；L8 → agent-context-db-uwu** | 通用核心可独立使用/发布；专有扩展强依赖 uwu crate 隔离变更；两者并行不干扰 |
| 定位深度 | 完全复刻 + 功能升级 | 与五维深度融合，吸收全部 P0/P1 设计 |
| 存储后端 | 独立双层存储（PG + Qdrant） | 不依赖 uwu_database，与 OpenViking AGFS+向量库对齐 |
| Sidecar 处理 | 重构为内嵌异步管线 | 删除 consolidator Sidecar，用 SemanticQueue 替代 |
| **wiki 子域实现** | **集成 uwu_wiki（去存储层）** | uwu_wiki 提供完整 LLM Wiki 工作流（Ingest/Query/Lint）+ Block 树 + CRDT；context-db 注入存储层，uwu_wiki 不自持存储；agent-wiki crate 直接删除 |
| LLM Provider | trait 抽象双实现 | `LlmClient` trait，MCP 默认 + 直连 SDK 备选 |
| HTTP 模式 | 第一版延后 | 聚焦引擎内嵌入式集成，降低首版复杂度 |
| 旧数据迁移 | 不需要 | uwu_agent_engine 仍在设计阶段，无生产数据 |

### 0.5 模块化交付分层（M0-M3，权威落地顺序）

> 本节是落地的**单一权威顺序**。§2 的 8 层（L1-L8）是*逻辑分层*，不是交付顺序；§9/§10 的 25 项功能是*能力池*，不是首版范围。任何实现从 M0 开始，逐层解锁，禁止跳阶。

**模块拆分原则**：每个 M 是一个可独立编译、可独立测试、可独立废弃的 crate 边界。上层依赖下层，下层不知上层存在。

| 里程碑 | crate | 交付内容 | 依赖 | 验收标准 |
|---|---|---|---|---|
| **M0 内核** | `agent-context-db-core` | `uwu://` URI + `FsOps` + `ContentRepo` 两个窄 trait + 单后端(PG) + L0/L1/L2 读 | 无 uwu 依赖 | 能写入/寻址/读取一个 `MemoryClass::Cases` 条目并 ls 出来 |
| **M1 检索** | `agent-context-db-retrieve` | `HierarchicalRetriever`（**仅依赖 `FsOps` 端口**）+ Qdrant 索引层 + 8 类 `MemoryExtractor` | M0 | retrieve_typed 返回按类过滤的命中；可用内存版 `FsOps` mock 单测,不启 PG |
| **M2 版本** | `agent-context-db-version` | `VersionOps` 端口实现(branch/tag/snapshot) + 子树快照 + 时间旅行 | M0 | fork 一个子树、改写、rollback 回原版本；version feature 关闭时 M0/M1 仍可编译 |
| **M3 uwu 扩展** | `agent-context-db-uwu` | 五维 bridge + Guard 写约束 + fork 推演 | M0-M2 + 全部 uwu crate | 用真实 `agent-state`/`agent-metacognition` 对接跑通 pred_error 归档 |

**内部解耦硬约束（编译期强制,见 §2.0）**：
- 每个 M crate 只 `use` 下层的**窄 trait**,禁止 `use` 下层具体 struct（如 `AgfsStore`/`QdrantIndex`）。后端类型只在 composition root(L2 service)装配时出现一次。
- `ContextStore` 聚合别名仅供最终应用层便利使用；M0-M3 库内部一律依赖 `FsOps`/`ContentRepo`/`VersionOps` 窄端口。
- 后端可替换性验收:M1 单测必须用内存版 `FsOps` 实现,证明检索层不绑定 PG。

**与已落地 crate 的对接策略（破坏性，但按阶隔离）**：
- M0-M2 期间 **不动** 任何现有 crate。`agent-state`/`agent-memory`/`agent-metacognition` 照常在内存里跑,context-db 仅作为可选持久化后端并行存在。
- **M3 才引入破坏性重构**（§5 清单）：`agent-memory` 降为薄适配层、`agent-wiki` 删除、`agent-sidecar-consolidator` 内嵌化。在 M0-M2 未验证前不得执行这些删除,否则一旦 context-db 设计证伪,回退成本极高。
- 真值源边界统一遵循 §6.3 的「事实层 / 派生层」原则:内存态(scalar / ring-buffer / 内存图)是热路径真值,context-db 是其冷归档与重算来源,而非反过来。

**首版明确不做**（砍掉,进 M4+ 能力池）：F16-F30 全部 15 项创新功能、CRDT 多 Agent 联邦、HTTP 模式、LLM 合并仲裁。首版只需 M0-M2 的单 Agent、单后端、嵌入式闭环。

---

## 1. 整体架构总览

### 1.1 层次定位图（破坏性重构后）

```
┌────────────────────────────────────────────────────────────────┐
│ Agent 决策层 (uwu 独占,保留)                                    │
│  Reaction → FlowGraph(P→M→R→E) → Metacognition → Guard        │
│  五维: Reaction/State/Metacog/Persona/Character                │
│  Task / Collaboration / LearnNode                              │
├────────────────────────────────────────────────────────────────┤
│ 专有扩展层 (agent-context-db-uwu，uwu 独占)                    │
│   五维 FS 映射 · Guard 写入约束 · State fork 推演               │
│   Metacog 校准检索 · LLM Wiki 工作流 (uwu_wiki)                │
├────────────────────────────────────────────────────────────────┤
│ 通用上下文管理层 (agent-context-db，可独立发布)                  │
│   uwu:// FS + L0/L1/L2 + 目录递归检索                          │
│   + 8 种记忆分类 + 轨迹/经验两层                                │
│   + 两阶段 commit + 异步语义管线 + MVCC + CRDT                 │
├────────────────────────────────────────────────────────────────┤
│ 双层存储 (agent-context-db 内部持有)                            │
│   AGFS 内容层 (PostgreSQL)  ←→  索引层 (Qdrant)                 │
│   (L0/L1/L2 完整内容+关联)     (URI+向量+元数据指针)             │
├────────────────────────────────────────────────────────────────┤
│ 基础设施 (复用)                                                 │
│   agent-mesh (事件网格) / uwu_logger / uwu_wasm                 │
└────────────────────────────────────────────────────────────────┘
```

### 1.2 crate 依赖图

```
                    ┌─────────────────────┐
                    │   agent-core       │  (会话管理+FlowGraph+能力注册表)
                    └──────────┬──────────┘
                               │ depends on
              ┌────────────────┼──────────────────────┐
              ▼                ▼                      ▼
      ┌───────────────┐ ┌──────────────┐  ┌──────────────────────┐
      │ agent-session │ │ agent-task    │  │ agent-context-db-uwu │ ← 专有扩展
      └──────┬────────┘ └──────────────┘  └──────────┬───────────┘
             │                                        │ depends on
   持有五维 ──┼── (State/Persona/Metacog/Character/Reaction)
             │                                        ▼
             │                            ┌────────────────────┐
             │                            │ agent-context-db   │ ← 通用核心
             │                            │ (L1-L7，可独立用)   │
             │                            └────────┬───────────┘
             ▼                                     ▼
      ┌──────────────────────────────────────────────────────┐
      │  agent-state / agent-persona / agent-metacognition   │
      │  agent-character / agent-reaction                    │
      │  (五维 crate 保留,但其持久化委托给 context-db-uwu)     │
      └──────────────────────────────────────────────────────┘
                             │
                             ▼
      ┌──────────────────────────────────────────────────────┐
      │  agent-mesh (事件网格)  agent-guard (五层闸门)          │
      │  uwu-crdt (合并计算层，不持久化)  agent-learning       │
      └──────────────────────────────────────────────────────┘
```

### 1.3 模块划分：通用核心（L1-L7）+ 专有扩展（L8）

```
┌──────────────────────────────────────────────────────────┐
│          agent-context-db（通用核心，可独立使用）           │
│                                                          │
│  L1 client/     ContextDbClient（嵌入式）                 │
│  L2 service/    ContextDbService · UriResolver           │
│  L3 retrieve/   HierarchicalRetriever · RAG · Reranker   │
│  L4 session/    SessionCompressor（两阶段 commit）         │
│  L5 parse/      SemanticProcessor · MemoryExtractor(8类) │
│  L6 compressor/ SemanticQueue（异步管线）                  │
│  L7 storage/    AgfsStore(PG) · VectorIndex(Qdrant) · FsOps │
│  L7 wiki/       WikiDomain（LLM Wiki 工作流）              │
│                   Ingest · Query（含反写）· Lint           │
│                   WikiStorage trait（来自 uwu_wiki）       │
│                   ContextDbWikiStorage（WikiStorage 实现） │
│                     复用 AgfsStore(PG) + VectorIndex(Qdrant)│
│                                                          │
│  公开 trait：ContextStore · LlmClient · HierarchicalRetriever │
│              WikiStorage · DocStore · OpLog              │
└────────────────────────┬─────────────────────────────────┘
                         │ depends on
┌────────────────────────▼─────────────────────────────────┐
│       agent-context-db-uwu（专有扩展，uwu 独占）            │
│                                                          │
│  L8 uwu/                                                 │
│    five_dim_bridge.rs   五维 FS 映射（State/Persona/...）  │
│    guard_integrator.rs  GuardLayer 写入约束               │
│    fork_feeder.rs       State fork 沙盒推演               │
│    metacog_bridge.rs    Metacog 校准检索                  │
│    character_constraint.rs Character 价值观约束           │
│                                                          │
│  依赖：agent-state · agent-persona · agent-metacognition  │
│        agent-character · agent-guard · uwu_wiki          │
└──────────────────────────────────────────────────────────┘
```

**分层边界原则：**

| 层 | 归属 | 外部依赖 | 可独立发布 |
|---|---|---|---|
| L1-L7（含 WikiDomain + ContextDbWikiStorage） | 通用核心（agent-context-db） | PG · Qdrant · tokio · uwu_wiki（wiki-core/llm） | ✓ |
| L8 五维/Guard/Fork | 专有扩展（agent-context-db-uwu） | 全部 uwu crate + agent-guard | ✗ |
| `WikiStorage` / `LlmClient` trait | 通用核心 | 零依赖 | ✓ |
| `ContextDbWikiStorage`（WikiStorage 实现） | **通用核心 L7**（复用 PG/Qdrant，无 uwu 依赖） | PG + Qdrant（L7 storage 已持有） | ✓ |

参考 OpenViking 七层架构，重构为八层（新增 uwu 升级层）：

| 层 | 模块路径 | 归属 | 职责 |
|---|---|---|---|
| L1 客户端层 | `agent_context_db::client` | 通用 | `ContextDbClient`（第一版仅嵌入式）、`ContextDbHandle` |
| L2 服务层 | `agent_context_db::service` | 通用 | `ContextDbService`（编排）、`UriResolver`、`PermissionGuard` |
| L3 检索层 | `agent_context_db::retrieve` | 通用 | `HierarchicalRetriever`、`IntentAnalyzer`、`Reranker`、`RetrievalTrace` |
| L4 会话层 | `agent_context_db::session` | 通用 | `SessionCompressor`（两阶段 commit）、`MessageArchiver` |
| L5 解析层 | `agent_context_db::parse` | 通用 | `SemanticProcessor`（自底向上 L0/L1）、`MemoryExtractor`（8 种分类 + LLM 去重）、`TrajectoryExtractor` |
| L6 压缩层 | `agent_context_db::compressor` | 通用 | `SemanticQueue`（异步任务队列）、`CompressionScheduler` |
| L7 存储层 | `agent_context_db::storage` | 通用 | `AgfsStore`(PG 内容层)、`VectorIndex`(Qdrant 索引层)、`FsOps`(ls/find/grep/read/tree) |
| L7 wiki 层 | `agent_context_db::wiki` | 通用 | `WikiDomain`（LLM Wiki 工作流：Ingest/Query/Lint）、`WikiStorage` trait、`ContextDbWikiStorage`（复用 L7 storage 的 PG/Qdrant，无 uwu 依赖） |
| L8 升级层 | `agent_context_db_uwu::uwu` | 专有 | `FiveDimBridge`（五维映射）、`CrdtMerger`、`GuardIntegrator`、`ForkFeeder`、`MetacogBridge`、`CharacterConstraint`、`LlmClient` 实现 |

---

## 2. 核心 trait 定义

### 2.0 内部模块化解耦原则（trait 分离 + 端口/适配器）

> 本节是**内部解耦的权威约束**。§2.2 起的各 trait 必须遵守以下规则,否则退化为单体。

**问题**：初版把 `ContextStore` 设计成一个胖 trait(FS + CRUD + MVCC + 租户 20+ 方法),这是 god-trait 反模式——任何后端实现都被迫一次性实现全部能力,无法只替换其中一层,也无法对单一职责做单测。

**解耦规则**：

1. **接口隔离(ISP)**：按职责把胖 trait 拆成窄 trait,调用方只依赖它真正用到的那个。`ContextStore` 拆为:
   - `FsOps`(ls/find/grep/tree/read) — 只读寻址
   - `ContentRepo`(write/delete/rename) — 内容写
   - `VersionOps`(version_history/rollback/diff) — 版本
   - `TenantOps`(list_tenants) — 租户
   胖 `ContextStore` 保留为 `FsOps + ContentRepo` 的 supertrait 别名,供便利调用,但内部各层只依赖窄 trait。

2. **端口/适配器(六边形)**：每层定义自己的**端口 trait**(它需要什么),不直接依赖下层具体类型。存储后端(PG/Qdrant)是**适配器**,实现端口。检索层依赖 `FsOps` 端口,不依赖 `AgfsStore` 具体类型——PG 可换成 SQLite/内存实现而检索层零改动。

3. **依赖倒置(DIP)**：`LlmClient`/`WikiStorage`/`VersionStore` 全部是 trait 注入,构造期由 composition root(L2 service)装配。任何层不得 `use` 另一层的具体 struct,只 `use` 其 trait。

4. **单向依赖**：L(n) 只能依赖 L(n-1) 的 trait,禁止反向或跨层。编译期由 crate 边界强制(见 §0.5 的 M0-M3 crate 拆分)。

**解耦收益对照**：

| 维度 | 胖 trait(前) | 窄 trait + 端口(后) |
|---|---|---|
| 替换 PG→SQLite | 改所有实现 | 只写新 `ContentRepo` 适配器 |
| 单测检索层 | 需 mock 20+ 方法 | 只 mock `FsOps` 5 方法 |
| M0 首版最小实现 | 必须实现全 trait | 只实现 `FsOps + ContentRepo` |
| 版本层可选 | 编译期强绑定 | `VersionOps` 独立 crate,feature 开关 |

### 2.1 LlmClient（LLM 调用抽象，trait 双实现）

```rust
// agent-context-db/src/uwu/llm.rs

use async_trait::async_trait;

/// LLM 调用统一抽象
/// 所有 SemanticProcessor / MemoryExtractor / IntentAnalyzer 的 LLM 调用都走此 trait
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 单轮文本补全（用于 L0 摘要、L1 概览生成）
    async fn complete(&self, prompt: &str, opts: &LlmOpts) -> Result<String, LlmError>;

    /// 结构化输出（用于记忆提取、去重决策、意图分析）
    /// 要求 LLM 返回可解析的 JSON
    async fn complete_json<T: serde::de::DeserializeOwned>(
        &self, prompt: &str, schema: &serde_json::Value, opts: &LlmOpts
    ) -> Result<T, LlmError>;

    /// 多模态输入（用于图片 L0/L1 文本描述）
    async fn describe_image(&self, image_url: &str, opts: &LlmOpts)
        -> Result<String, LlmError>;
}

#[derive(Debug, Clone, Default)]
pub struct LlmOpts {
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub timeout_ms: Option<u64>,
    pub model: Option<String>,  // None 用默认模型
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("network: {0}")]
    Network(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),
    #[error("provider: {0}")]
    Provider(String),
}

// === 双实现 ===

/// 实现1：通过 MCP 协议调用 LLM（默认，与引擎其他 LLM 调用一致）
pub struct McpLlmClient {
    tool_executor: Arc<dyn agent_tools::ToolExecutor>,
    server_id: String,  // MCP Server ID
}

/// 实现2：直连 SDK（OpenAI/Anthropic，备用/低延迟场景）
pub struct DirectLlmClient {
    provider: DirectProvider,
    api_key: String,
    api_base: String,
}

pub enum DirectProvider { OpenAI, Anthropic, Volcengine, Azure }
```

**设计要点**：
- `McpLlmClient` 为默认实现，通过 `agent-tools` MCP 协议调用，与引擎其他 LLM 调用路径一致（可插拔、可审计、经 GuardLayer）。
- `DirectLlmClient` 为备用实现，直连 OpenAI/Anthropic SDK，适合对延迟敏感的场景（如 L0 摘要生成要求 < 500ms）。
- 配置选择：

```toml
[context_db.llm]
default_impl = "mcp"  # 或 "direct"
mcp.server_id = "uwu-llm-server"
direct.provider = "openai"
direct.api_base = "https://api.openai.com/v1"
```

### 2.2 ContextStore（存储层核心 trait）

```rust
// agent-context-db/src/storage/store.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 上下文条目唯一标识（uwu:// URI 的强类型封装）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextUri(pub String);  // 例 "uwu://tenant1/user/u1/memories/preferences/p1"

impl ContextUri {
    pub fn parent(&self) -> Option<ContextUri> { /* ... */ }
    pub fn depth(&self) -> usize { /* ... */ }
    pub fn category(&self) -> UriCategory { /* 解析路径段 */ }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UriCategory { User, Agent, Resources, Skills, Wiki, Sessions, State, Persona, Metacog, Character }

/// 三层信息模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub uri: ContextUri,
    pub tenant: TenantId,
    pub l0_abstract: String,         // ~100 tokens, Markdown
    pub l1_overview: Option<String>, // ~2k tokens, Markdown, 含章节导航
    pub l2_detail_uri: Option<ContentRef>, // 指向原始内容（多模态）
    pub content_type: ContentType,   // Text / Image / Audio / Video / Binary
    pub metadata: ContextMeta,
    pub mvcc_version: MvccVersion,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMeta {
    pub memory_class: Option<MemoryClass>,  // 8 种分类,Resource/Wiki/State 为 None
    pub state_scope: Option<StateScope>,     // Short/Mid/Long（仅 State 条目）
    pub persona_node: Option<PersonaNodeId>,
    pub character_constraint: Option<CharacterConstraintId>,
    pub tags: Vec<String>,
    pub custom: serde_json::Value,
}

/// 8 种记忆分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryClass {
    // user 类
    Profile,      // 身份/属性,可合并
    Preferences, // 偏好,可合并
    Entities,    // 人/项目,可合并
    Events,      // 事件/决策,不可合并
    // agent 类
    Cases,       // 问题+解决方案,不可合并
    Patterns,    // 可复用流程,可合并
    Tools,       // 工具使用经验,可合并
    Skills,      // 技能执行经验,可合并
}

impl MemoryClass {
    pub fn mergeable(&self) -> bool {
        matches!(self, Self::Profile | Self::Preferences | Self::Entities
                          | Self::Patterns | Self::Tools | Self::Skills)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateScope { Short, Mid, Long }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MvccVersion(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub Uuid);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRef(pub Uuid); // AGFS 内容 blob ID

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType { Text, Image, Audio, Video, Binary }

/// 存储层核心 trait —— 按职责拆分为窄 trait（接口隔离原则）
/// 各层只依赖它真正用到的窄 trait；下面四个是独立职责端口。

/// 端口 1：只读 FS 寻址（检索层唯一依赖此端口）
#[async_trait]
pub trait FsOps: Send + Sync {
    async fn ls(&self, dir: &ContextUri) -> Result<Vec<DirEntry>, ContextError>;
    async fn find(&self, pattern: &FindPattern) -> Result<Vec<ContextUri>, ContextError>;
    async fn grep(&self, regex: &str, scope: &ContextUri) -> Result<Vec<GrepHit>, ContextError>;
    async fn tree(&self, root: &ContextUri, depth: usize) -> Result<TreeNode, ContextError>;
    async fn read(&self, uri: &ContextUri, level: ContentLevel) -> Result<ContentPayload, ContextError>;
}

/// 端口 2：内容写（M0 必需）
#[async_trait]
pub trait ContentRepo: Send + Sync {
    async fn write(&self, entry: ContextEntry) -> Result<MvccVersion, ContextError>;
    async fn delete(&self, uri: &ContextUri) -> Result<(), ContextError>;
    async fn rename(&self, from: &ContextUri, to: &ContextUri) -> Result<(), ContextError>;
}

/// 端口 3：版本操作（M2 独立 crate，feature 开关；M0/M1 可不实现）
#[async_trait]
pub trait VersionOps: Send + Sync {
    async fn version_history(&self, uri: &ContextUri) -> Result<Vec<VersionEntry>, ContextError>;
    async fn rollback(&self, uri: &ContextUri, to: MvccVersion) -> Result<(), ContextError>;
    async fn diff(&self, uri: &ContextUri, a: MvccVersion, b: MvccVersion) -> Result<ContextDiff, ContextError>;
}

/// 端口 4：租户隔离
#[async_trait]
pub trait TenantOps: Send + Sync {
    async fn list_tenants(&self) -> Result<Vec<TenantId>, ContextError>;
}

/// 便利 supertrait 别名：需要"完整存储"能力的调用方用它，
/// 但库内部各层严禁依赖此聚合 trait，只依赖上面的窄端口。
pub trait ContextStore: FsOps + ContentRepo + VersionOps + TenantOps {}
impl<T: FsOps + ContentRepo + VersionOps + TenantOps> ContextStore for T {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentLevel { L0, L1, L2 }

#[derive(Debug, Clone)]
pub enum ContentPayload {
    Abstract(String),  // L0
    Overview(String),  // L1
    Detail(Vec<u8>),   // L2 原始字节（多模态）
}
```

### 2.3 HierarchicalRetriever（检索层 trait）

```rust
// agent-context-db/src/retrieve/retriever.rs

/// 分层检索器：意图分析 → 目录递归 → Rerank
#[async_trait]
pub trait HierarchicalRetriever: Send + Sync {
    async fn retrieve(&self, query: &str, ctx: &RetrieveContext)
        -> Result<RetrievalResult, ContextError>;

    async fn retrieve_typed(&self, query: &str, class: MemoryClass, ctx: &RetrieveContext)
        -> Result<RetrievalResult, ContextError>;
}

#[derive(Debug, Clone, Default)]
pub struct RetrieveContext {
    pub tenant: Option<TenantId>,
    pub user_id: Option<String>,
    pub agent_id: Option<String>,
    pub budget_tokens: Option<usize>,
    pub prefer_level: ContentLevel,
    pub state_scope_hint: Option<StateScope>,
    pub trace_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub hits: Vec<RetrievalHit>,
    pub trace: RetrievalTrace,     // 检索轨迹可视化
    pub tokens_used: usize,
    pub intent: Vec<TypedQuery>,   // 解析出的意图
}

#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub uri: ContextUri,
    pub level: ContentLevel,
    pub content: ContentPayload,
    pub relevance: f32,
    pub parent_chain: Vec<ContextUri>, // 父目录链（递归深入路径）
}

/// 检索轨迹：每次 ls/find 路径完整保留
#[derive(Debug, Clone, Default)]
pub struct RetrievalTrace {
    pub steps: Vec<TraceStep>,
}

#[derive(Debug, Clone)]
pub enum TraceStep {
    IntentAnalysis { raw: String, typed: Vec<TypedQuery> },
    InitialLocate { query: TypedQuery, top_dirs: Vec<(ContextUri, f32)> },
    IntraDirSearch { dir: ContextUri, candidates: Vec<ContextUri> },
    RecursiveDescent { from: ContextUri, into: ContextUri, reason: String },
    Rerank { input: usize, kept: usize, model: String },
    Load { uri: ContextUri, level: ContentLevel, tokens: usize },
}

/// 意图分析 trait：将自然语言查询拆为 0-5 个类型化查询
#[async_trait]
pub trait IntentAnalyzer: Send + Sync {
    async fn analyze(&self, query: &str, ctx: &RetrieveContext)
        -> Result<Vec<TypedQuery>, ContextError>;
}

#[derive(Debug, Clone)]
pub struct TypedQuery {
    pub kind: QueryKind,
    pub text: String,
    pub target_dirs: Vec<ContextUri>,
    pub expected_class: Option<MemoryClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    SemanticSearch, EntityLookup, EventRecall, SkillReuse,
    PatternMatch, StateSnapshot, PersonaRelation,
}

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(&self, query: &str, hits: Vec<RetrievalHit>)
        -> Result<Vec<RetrievalHit>, ContextError>;
}
```

### 2.4 SessionCompressor（会话压缩 trait）

```rust
// agent-context-db/src/session/compressor.rs

/// 两阶段 commit 会话压缩器
#[async_trait]
pub trait SessionCompressor: Send + Sync {
    /// Phase1 同步：归档消息 + 清空当前 + 返回 task_id
    async fn commit_phase1(&self, session: &SessionHandle)
        -> Result<CommitTaskId, ContextError>;

    /// Phase2 异步：生成 L0/L1 + 提取记忆 + 写 memory_diff.json
    async fn commit_phase2(&self, task_id: CommitTaskId)
        -> Result<CommitResult, ContextError>;

    /// 查询异步任务状态
    async fn poll_task(&self, task_id: CommitTaskId) -> Result<TaskStatus, ContextError>;
}

#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub session_id: Uuid,
    pub tenant: TenantId,
    pub user_id: String,
    pub agent_id: String,
    pub messages: Vec<SessionMessage>,
    pub compression_index: u64,
    pub archive_dir: ContextUri,
}

#[derive(Debug, Clone)]
pub struct SessionMessage {
    pub role: Role,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum Role { User, Assistant, Tool, System }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommitTaskId(pub Uuid);

#[derive(Debug, Clone)]
pub enum TaskStatus { Pending, Processing, Done(DoneMarker), Failed(String) }

#[derive(Debug, Clone)]
pub struct DoneMarker {
    pub task_id: CommitTaskId,
    pub finished_at: chrono::DateTime<chrono::Utc>,
    pub abstract_uri: ContextUri,
    pub overview_uri: ContextUri,
    pub memory_diff_uri: Option<ContextUri>,
}

/// 记忆变更审计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryDiff {
    pub adds: Vec<MemoryChange>,
    pub updates: Vec<MemoryChange>,
    pub deletes: Vec<MemoryChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChange {
    pub uri: ContextUri,
    pub class: MemoryClass,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub reason: String,
}
```

### 2.5 SemanticProcessor + MemoryExtractor + TrajectoryExtractor（解析层）

```rust
// agent-context-db/src/parse/processor.rs

/// 语义处理器：自底向上生成 L0/L1
#[async_trait]
pub trait SemanticProcessor: Send + Sync {
    async fn generate_abstract(&self, uri: &ContextUri) -> Result<String, ContextError>;
    async fn generate_overview(&self, uri: &ContextUri) -> Result<String, ContextError>;
    async fn aggregate_upward(&self, root: &ContextUri) -> Result<(), ContextError>;
    async fn multimodal_to_text(&self, uri: &ContextUri) -> Result<(String, String), ContextError>;
}

/// 记忆提取器：8 种分类 + LLM 去重
#[async_trait]
pub trait MemoryExtractor: Send + Sync {
    async fn extract(&self, archive: &ContextUri) -> Result<Vec<MemoryCandidate>, ContextError>;
    async fn deduplicate(&self, candidates: Vec<MemoryCandidate>)
        -> Result<Vec<DedupDecision>, ContextError>;
}

#[derive(Debug, Clone)]
pub struct MemoryCandidate {
    pub class: MemoryClass,
    pub content: String,
    pub source_uri: ContextUri,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct DedupDecision {
    pub candidate: MemoryCandidate,
    pub action: CandidateAction,
    pub merge_target: Option<ContextUri>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateAction { Skip, Create, Merge, Delete, None }

/// 轨迹提取器：会话级 → Trajectory；多轨迹 → Experience
#[async_trait]
pub trait TrajectoryExtractor: Send + Sync {
    async fn extract_trajectory(&self, archive: &ContextUri) -> Result<Trajectory, ContextError>;
    async fn induce_experience(&self, trajectories: Vec<ContextUri>)
        -> Result<Experience, ContextError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    pub uri: ContextUri,
    pub session_id: Uuid,
    pub did_what: String,
    pub how: String,
    pub result: String,
    pub state_snapshot_uri: Option<ContextUri>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub uri: ContextUri,
    pub situation: String,   // 场景描述
    pub approach: String,    // 采取方法
    pub reflect: String,     // 反思
    pub related_trajectories: Vec<ContextUri>,
}
```

### 2.6 SemanticQueue（压缩层 trait，替代 Sidecar）

```rust
// agent-context-db/src/compressor/queue.rs

/// 异步语义处理队列（替换 agent-sidecar-consolidator）
#[async_trait]
pub trait SemanticQueue: Send + Sync {
    async fn enqueue(&self, task: SemanticTask) -> Result<TaskId, ContextError>;
    async fn dequeue(&self) -> Result<Option<SemanticTask>, ContextError>;
    fn spawn_worker(&self, parallelism: usize) -> WorkerHandle;
    async fn subscribe_done(&self) -> tokio::sync::mpsc::Receiver<TaskDoneEvent>;
}

#[derive(Debug, Clone)]
pub enum SemanticTask {
    GenerateAbstract(ContextUri),
    GenerateOverview(ContextUri),
    AggregateUpward(ContextUri),
    ExtractMemories { archive: ContextUri, session: SessionHandle },
    DeduplicateMemories(Vec<MemoryCandidate>),
    ExtractTrajectory(ContextUri),
    InduceExperience(Vec<ContextUri>),
    MultimodalToText(ContextUri),
}

pub struct WorkerHandle {
    pub shutdown: tokio::sync::oneshot::Sender<()>,
}

#[derive(Debug, Clone)]
pub struct TaskDoneEvent {
    pub task_id: TaskId,
    pub task: SemanticTask,
    pub result: TaskOutcome,
}

#[derive(Debug, Clone)]
pub enum TaskOutcome { Success, PartialFailure(String), Failure(String) }
```

---

### ContextStore trait 升级

v1 的 `ContextStore` trait 中 MVCC 方法替换为 `VersionStore` 委托：

```rust
// agent-context-db/src/storage/store.rs (v2)

#[async_trait]
pub trait ContextStore: Send + Sync {
    // === FS 操作（不变）===
    async fn ls(&self, dir: &ContextUri) -> Result<Vec<DirEntry>, ContextError>;
    async fn find(&self, pattern: &FindPattern) -> Result<Vec<ContextUri>, ContextError>;
    async fn grep(&self, regex: &str, scope: &ContextUri) -> Result<Vec<GrepHit>, ContextError>;
    async fn tree(&self, root: &ContextUri, depth: usize) -> Result<TreeNode, ContextError>;
    async fn read(&self, uri: &ContextUri, level: ContentLevel) -> Result<ContentPayload, ContextError>;

    // === CRUD（升级：写操作产生 commit）===
    async fn write(&self, entry: ContextEntry, message: &str)
        -> Result<CommitId, ContextError>;  // ★ 返回 CommitId 而非 MvccVersion
    async fn delete(&self, uri: &ContextUri, message: &str)
        -> Result<CommitId, ContextError>;
    async fn rename(&self, from: &ContextUri, to: &ContextUri, message: &str)
        -> Result<CommitId, ContextError>;

    // === 版本管理（委托 VersionStore）===
    fn versions(&self) -> &dyn VersionStore;  // ★ 获取版本管理接口

    // === ASOF 读取 ===
    async fn read_at(&self, uri: &ContextUri, ref_: VersionRef, level: ContentLevel)
        -> Result<ContentPayload, ContextError>;

    // === 快照 ===
    async fn snapshot(&self, scope: &ContextUri, name: &str) -> Result<SnapshotId, ContextError>;
    async fn restore_snapshot(&self, snapshot: &SnapshotId) -> Result<CommitId, ContextError>;

    // === PubSub ===
    fn pubsub(&self) -> &dyn ContextPubSub;

    // === ACL ===
    async fn check_permission(&self, uri: &ContextUri, principal: &Principal, perm: Permissions)
        -> Result<bool, ContextError>;

    // === Pack ===
    fn pack(&self) -> &dyn ContextPackOps;

    // === 生命周期 ===
    async fn apply_lifecycle(&self, policy: &LifecyclePolicy) -> Result<LifecycleReport, ContextError>;
}
```

---

### LlmClient 升级：流式+批量+投机


```rust
// agent-context-db/src/uwu/llm.rs (v3 升级)

#[async_trait]
pub trait LlmClient: Send + Sync {
    // v1/v2 方法保留...

    /// v3 新增：流式生成（L0/L1 边生成边写入，降低首字节延迟）
    async fn stream_complete(
        &self, prompt: &str, opts: &LlmOpts
    ) -> Result<Box<dyn LlmStream>, LlmError>;

    /// v3 新增：批量调用（多个摘要请求合并为一次 LLM 调用）
    async fn batch_complete(
        &self, prompts: &[String], opts: &LlmOpts
    ) -> Result<Vec<String>, LlmError>;

    /// v3 新增：投机执行（同时发起大模型+小模型，小模型先出就用小模型结果）
    async fn speculative_complete(
        &self, prompt: &str, opts: &LlmOpts
    ) -> Result<String, LlmError>;
}

#[async_trait]
pub trait LlmStream: Send + Sync {
    async fn next_chunk(&mut self) -> Option<Result<String, LlmError>>;
}

/// 投机执行器：大模型和小模型并行，小模型快则用小模型，大模型验证
pub struct SpeculativeExecutor {
    large: Arc<dyn LlmClient>,
    small: Arc<dyn LlmClient>,
}

impl SpeculativeExecutor {
    async fn speculative_complete(&self, prompt: &str) -> String {
        let large_fut = self.large.complete(prompt, &opts);
        let small_fut = self.small.complete(prompt, &opts);
        tokio::select! {
            Ok(small_result) = small_fut => {
                // 小模型先返回，先用，大模型结果作为验证
                // 如果大模型后续返回差异大，触发修正
                small_result
            }
            Ok(large_result) = large_fut => {
                // 大模型先返回（小模型超时），直接用
                large_result
            }
        }
    }
}
```


## 3. uwu FS 范式设计

### 3.1 URI 协议

**自定义协议**：`uwu://`（不沿用 `viking://`）

```
uwu://<tenant>/<scope>/<entity_path>[/<sub_path>][/<leaf>]

tenant    ::= account_id (多租户第一级)
scope     ::= user | agent | resources | wiki | sessions
entity_path ::= 由 scope 决定
```

### 3.2 虚拟 FS 目录结构

```
uwu://
├── {tenant}/                                # 多租户第一级 (account)
│   │
│   ├── user/{user_id}/                      # 用户域
│   │   ├── memories/                         # 8 种记忆分类
│   │   │   ├── profile/                      # 身份/属性 (可合并)
│   │   │   │   └── {entry_id}/
│   │   │   │       ├── .abstract.md          # L0
│   │   │   │       ├── .overview.md          # L1
│   │   │   │       └── content.md            # L2
│   │   │   ├── preferences/                  # 偏好 (可合并)
│   │   │   ├── entities/                     # 人/项目 (可合并)
│   │   │   └── events/                       # 事件/决策 (不可合并)
│   │   ├── resources/                        # 用户私有资源
│   │   │   └── {project_name}/{docs,src}/
│   │   ├── skills/                           # 用户级技能
│   │   └── peers/{visitor_id}/               # 协作伙伴上下文
│   │       ├── memories/
│   │       └── resources/
│   │
│   ├── agent/{agent_id}/                     # Agent 域
│   │   ├── memories/                         # 8 种记忆分类
│   │   │   ├── cases/                        # 问题+解决方案 (不可合并)
│   │   │   ├── patterns/                     # 可复用流程 (可合并)
│   │   │   ├── tools/                        # 工具使用经验 (可合并)
│   │   │   ├── skills/                       # 技能执行经验 (可合并)
│   │   │   ├── trajectories/                 # ★ 轨迹层 (Layer1)
│   │   │   │   └── {tid}/
│   │   │   │       ├── .abstract.md
│   │   │   │       ├── .overview.md
│   │   │   │       └── trajectory.json
│   │   │   └── experiences/                  # ★ 经验层 (Layer2)
│   │   │       └── {eid}/
│   │   │           ├── .abstract.md
│   │   │           ├── .overview.md
│   │   │           └── experience.json       # Situation/Approach/Reflect
│   │   ├── persona/                          # ★ uwu 升级:Persona 一等公民
│   │   │   ├── identity.md                   # 当前身份 L2
│   │   │   ├── relations/                    # 关系图
│   │   │   │   ├── .graph.overview.md        # L1 关系图邻接表
│   │   │   │   └── {target_user}/
│   │   │   │       ├── .abstract.md          # L0 关系摘要
│   │   │   │       └── relation.md           # L2 关系详情 + 历史
│   │   │   └── resume/                       # 履历
│   │   ├── state/                            # ★ uwu 升级:State 一等公民
│   │   │   ├── short/                        # 短程 WS
│   │   │   │   └── {snapshot_id}/
│   │   │   │       ├── .abstract.md
│   │   │   │       └── state.json
│   │   │   ├── mid/                          # 中程 WS
│   │   │   └── long/                         # 长程 WS
│   │   ├── metacog/                          # ★ uwu 升级:Metacog 校准数据
│   │   │   ├── pred_errors/                  # CalibrationHistory 冷归档 + 派生标量重算来源
│   │   │   │   └── {ts}.json                 #   (非原始事实流;每条 = CalibrationRecord evict 后落盘)
│   │   │   └── cost_history/                # 成本历史
│   │   └── character/                        # ★ uwu 升级:Character 价值观
│   │       ├── core_values.md                # 不可变 (写入受限)
│   │       └── preferences.md                # 可调偏好
│   │
│   ├── resources/                            # 共享资源域
│   │   └── {project}/
│   │       ├── docs/
│   │       ├── src/
│   │       └── .abstract.md                  # 项目级 L0
│   │
│   ├── wiki/                                 # ★ wiki 子域 (由 uwu_wiki 驱动，去存储层后集成)
│   │   └── {space}/
│   │       ├── index.md                      # wiki 目录页（由 LLM Wiki Ingest 维护）
│   │       ├── ingest_log.jsonl              # Ingest 操作日志（由 LLM Wiki 工作流维护）
│   │       └── {doc_id}/
│   │           ├── .abstract.md              # L0 摘要（Block 树自动生成）
│   │           ├── .overview.md              # L1 导航（Block 路径树）
│   │           ├── blocks.json               # L2 Block 树完整内容（PG 为唯一真相源，此为 FS 视图）
│   │           └── versions/                 # MVCC 版本历史（Block 级，PG 持久化）
│   │               └── {v}/
│   │                   └── blocks.json
│   │           # ★ CRDT Op 日志不再以文件存储，由 WikiStorage::op_log() → PG op_log 表持久化
│   │
│   └── sessions/                             # ★ 会话归档
│       └── {session_id}/
│           └── archive/
│               └── {compression_index}/
│                   ├── messages.jsonl
│                   ├── .abstract.md          # Phase2 生成
│                   ├── .overview.md
│                   ├── memory_diff.json      # 审计日志
│                   └── .done                  # 完成标记
```

### 3.3 L0/L1/L2 在五维语境下的语义

| 层 | uwu 语义 | 生成时机 | Token 预算 |
|---|---|---|---|
| L0 abstract | 条目核心摘要：State=状态类型+主键、Persona=关系一句话、Memory=类别+主语 | 写入时同步/异步补全 | ~100 |
| L1 overview | 章节导航 + 核心字段：State=变更摘要+影响域、Persona=关系图邻接表、Memory=三段式预览 | 自底向上异步聚合 | ~2k |
| L2 detail | 原始完整内容：State=完整 JSON、Persona=完整关系图、Memory=原始对话片段、Wiki=完整 Markdown | 直接写入 | 不限 |

### 3.4 五维 State 存入 FS + MVCC 版本表达

**短/中/长程 WS ↔ FS 层级映射**：

| State 维度 | FS 路径 | L0/L1/L2 含义 |
|---|---|---|
| 短程 WS（当前 turn） | `uwu://.../agent/{id}/state/short/` | L0=状态类型+主键；L1=变更字段摘要；L2=完整 JSON |
| 中程 WS（会话内） | `uwu://.../agent/{id}/state/mid/` | L0=会话摘要；L1=最近 N 轮变更导航；L2=完整 WS |
| 长程 WS（跨会话） | `uwu://.../agent/{id}/state/long/` | L0=长期事实；L1=章节化长期状态；L2=完整长期数据 |

**MVCC 版本在 FS 表达**：

```
uwu://.../agent/{id}/state/mid/{snapshot_id}/
├── .abstract.md           # L0 (基于当前版本)
├── .overview.md           # L1 (基于当前版本)
├── content.json           # L2 当前版本 (mvcc_version=N)
└── versions/              # ★ 版本历史目录
    ├── v1/content.json
    ├── v2/content.json
    └── ...
```

每次 `ContextStore.write` 时：
1. 写入新版本到 `versions/v{N+1}/content.json`
2. 更新 `content.json` 软链接（PG 内部为最新指针）
3. 触发 SemanticQueue 异步重生成 L0/L1（基于新版本）

### 3.5 Persona 关系图存储

```
uwu://.../agent/{id}/persona/relations/
├── .graph.overview.md      # L1: 邻接表 + 关系类型聚合
├── alice/
│   ├── .abstract.md        # L0: "用户 alice，协作 3 次，信任 0.8"
│   └── relation.md         # L2: 完整关系历史 + 事件
└── bob/
    └── ...
```

图谱查询：`HierarchicalRetriever.retrieve_typed(query, PersonaRelation)` 先读 `.graph.overview.md`（L1 邻接表）定位相关节点，再 `read(L2)` 获取详情。

---

### FS 目录结构升级

```
uwu://.../agent/{id}/
├── state/mid/
│   ├── content.json              # HEAD 指向的当前版本
│   ├── .abstract.md
│   ├── .overview.md
│   └── .version/                 # ★ v2 版本元数据
│       ├── HEAD                  # "refs/heads/main"
│       ├── refs/
│       │   ├── heads/
│       │   │   ├── main          # → CommitId
│       │   │   ├── fork-explore-tot
│       │   │   └── fork-explore-cot
│       │   └── tags/
│       │       ├── stable        # mutable tag
│       │       └── milestone-q3  # immutable tag
│       ├── commits/
│       │   └── {commit_id}.json
│       ├── trees/
│       │   └── {tree_hash}/      # 内容寻址存储
│       └── snapshots/
│           └── {snapshot_id}.json
│
├── wiki/{space}/{page_id}/
│   ├── content.md
│   ├── .abstract.md
│   ├── .overview.md
│   └── .version/
│       ├── HEAD                  # "refs/heads/main"
│       ├── refs/heads/
│       │   ├── main
│       │   ├── agent-A-edit      # Agent A 的编辑分支
│       │   └── agent-B-edit      # Agent B 的编辑分支
│       ├── commits/
│       └── trees/
│
├── memories/preferences/
│   └── {entry_id}/
│       ├── content.md
│       ├── .abstract.md
│       └── .version/
│           ├── HEAD              # "refs/heads/main"
│           └── commits/          # LinearMvcc 策略：单线
│
├── .acl/                         # ★ v2 访问控制
│   └── rules.jsonl
├── .lifecycle/                   # ★ v2 生命周期策略
│   └── policies.json
├── .inheritance/                 # ★ v2 继承链
│   └── chain.json
└── .templates/                   # ★ v2 模板
    └── {template_id}.json
```

---

## 4. 版本管理系统

### 4.0 优化前 → 优化后 变化全景

| 维度 | 优化前 | 优化后 |
|---|---|---|
| **版本模型** | 线性 MVCC（`MvccVersion(u64)` 单调递增��� | **类 Git 有向无环图（DAG）**：commit + branch + tag + merge |
| **分支能力** | 无 | **命名分支 + fork 推演沙盒**：每个 State fork 即一个分支 |
| **快照能力** | State checkpoint（单点） | **子树快照（Subtree Snapshot）**：任意目录树的时间点冻结 |
| **时间旅行** | rollback 到指定版本（单条目） | **ASOF 查询**：任意时间点整树还原 + 差量回放 |
| **冲突合并** | 无（单写者） | **三路合并 + CRDT 自动合并 + LLM 辅助语义合并** |
| **标签** | 无 | **命名标签 + 语义标签**（里程碑标记） |
| **变更追踪** | memory_diff.json（会话级） | **变更事件流（ChangeLog）+ 因果链（Provenance）** |
| **版本策略** | 全条目统一 MVCC | **按上下文类型差异化版本策略**（Event 不可变/Wiki 可分支/Memory 可合并） |
| **导出/导入** | 无 | **ContextPack**：子树导出/导入/打包分享 |
| **访问控制** | 租户隔离 | **路径级 ACL + 版本级权限**（可回滚到某版本需权限） |

### 0.2 新增功能清单

| # | 功能 | 所属域 | 价值 |
|---|---|---|---|
| F1 | 类 Git 分支化版本管理 | 版本管理 | State fork 推演、多策略并行探索 |
| F2 | 子树快照 + 时间旅行 | 版本管理 | 任意时间点还原，调试/审计/回滚 |
| F3 | 三路合并 + LLM 语义合并 | 版本管理 | 多 Agent 协作编辑冲突解决 |
| F4 | 命名标签 + 语义标签 | 版本管理 | 里程碑标记，快速定位 |
| F5 | 变更事件流 + 因果链 | 版本管理 | 审计、溯源、影响分析 |
| F6 | 差异化版本策略 | 版本管理 | 按类型优化存储和语义 |
| F7 | ContextPack 导出导入 | 互操作 | Agent 上下文打包/迁移/分享 |
| F8 | 路径级 ACL + 版本权限 | 安全 | 精细化访问控制 |
| F9 | 上下文订阅与增量推送 | 实时 | Agent 订阅特定目录变更 |
| F10 | 上下文 TTL 与生命周期 | 存储 | 自动过期/归档/降级 |
| F11 | 上下文继承与覆盖 | 组织 | 类面向对象的上下文继承链 |
| F12 | 上下文模板与实例化 | 工程化 | 预定义上下文模板快速创建 |
| F13 | 上下文质量评分 | 元认知 | 检索结果质量打分反馈 |
| F14 | 上下文去重与相似度聚类 | 存储 | 跨 Agent/跨会话记忆去重 |
| F15 | 上下文血缘图 | 可观测性 | 可视化上下文产生/演化链路 |

---

### 1.1 版本模型重构

**v1 的线性 MVCC**：

```
v1 → v2 → v3 → v4 → v5 (current)
```

**v2 的 DAG**：

```
main:    v1 → v2 → v3 ─────────── v7 (merge) → v8 (current)
                         \         /
fork-A:                   → v4 → v5
fork-B:                   → v6 ────┘
tag: stable@v3, milestone-2026@v7
```

### 1.2 核心 API

```rust
// agent-context-db/src/version/model.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

/// 内容寻址哈希（类 Git SHA）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub String);  // blake3 哈希

/// 版本号（v2 废弃 v1 的 MvccVersion(u64)，改为 CommitId）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitId(pub Uuid);

/// 提交：版本图中的一个节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: CommitId,
    pub parents: Vec<CommitId>,       // ★ DAG：可有多个 parent（merge commit）
    pub tree_hash: ContentHash,       // 指向该版本的完整目录树快照
    pub author: Author,
    pub message: String,              // 类 commit message
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: CommitMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub system: bool,  // 系统自动提交（如 SemanticQueue）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMeta {
    pub trigger: CommitTrigger,       // 提交触发源
    pub changes: ChangeSet,           // 本次变更集合
    pub provenance: Vec<ProvenanceLink>, // 因果链
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommitTrigger {
    SessionCommit { session_id: Uuid, compression_index: u64 },
    AgentWrite { agent_id: String, action: String },
    ForkPromotion { fork_name: String },
    Merge { branches: Vec<BranchName> },
    AutoConsolidation,
    UserExplicit,
}

/// 变更集：本次提交相对 parent 的变更
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangeSet {
    pub adds: Vec<ContextUri>,
    pub updates: Vec<UriChange>,
    pub deletes: Vec<ContextUri>,
    pub renames: Vec<RenameOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UriChange {
    pub uri: ContextUri,
    pub old_hash: Option<ContentHash>,
    pub new_hash: ContentHash,
    pub diff_summary: String,  // L0 级别变更摘要
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameOp {
    pub from: ContextUri,
    pub to: ContextUri,
}

/// 因果链：这个提交是怎么来的
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceLink {
    pub source_uri: ContextUri,      // 触发源（如某条会话归档）
    pub source_commit: CommitId,     // 源提交
    pub relation: ProvenanceRelation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProvenanceRelation {
    DerivedFrom,    // 从源推导而来
    ExtractedFrom,  // 从源提取（如记忆从会话提取）
    MergedFrom,     // 从源合并
    ForkedFrom,     // 从源 fork
    TriggeredBy,    // 被源触发
}
```

### 1.3 分支模型

```rust
// agent-context-db/src/version/branch.rs

/// 命名分支
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: BranchName,
    pub head: CommitId,               // 分支头指针
    pub created_from: CommitId,       // 分支起点
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub branch_type: BranchType,
    pub lifecycle: BranchLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchName(pub String);  // "main" / "fork-state-abc" / "experiment-tot"

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BranchType {
    Main,              // 主线
    StateFork,         // State 推演沙盒（fork() 创建）
    Experiment,        // 实验性策略探索
    Collaboration,     // 多 Agent 协作分支
    Staging,           // 暂存（待审核合并）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BranchLifecycle {
    Active,
    Merged { into: BranchName, at: CommitId },
    Abandoned,
    Archived,  // 冻结只读
}
```

### 1.4 标签模型

```rust
// agent-context-db/src/version/tag.rs

/// 命名标签
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: TagName,
    pub target: CommitId,
    pub tag_type: TagType,
    pub message: String,
    pub created_by: Author,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagName(pub String);  // "stable" / "milestone-2026-q3" / "before-cleanup"

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TagType {
    /// 固定标签：指向特定 commit，永不移动
    Immutable,
    /// 移动标签：可重新指向（如 "stable" 总指向最新稳定版）
    Mutable,
    /// 语义标签：带语义条件，自动评估是否移动
    Semantic { condition: SemanticCondition },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticCondition {
    /// 例如："最近 100 次检索平均 relevance > 0.8 且无 GuardViolation"
    pub metric: String,
    pub threshold: f32,
    pub window_size: usize,
}
```

### 1.5 FS 中的版本结构表达

```
uwu://.../agent/{id}/
├── state/mid/
│   ├── content.json              # main 分支当前版本 L2
│   ├── .abstract.md              # main 分支当前 L0
│   ├── .overview.md              # main 分支当前 L1
│   └── .version/                 # ★ 版本元数据目录
│       ├── HEAD                  # 当前分支指针: "refs/heads/main"
│       ├── refs/
│       │   ├── heads/
│       │   │   ├── main          # → CommitId
│       │   │   ├── fork-state-abc # → CommitId (State fork 沙盒)
│       │   │   └── experiment-tot # → CommitId
│       │   └── tags/
│       │       ├── stable        # → CommitId
│       │       └── milestone-q3  # → CommitId
│       ├── commits/
│       │   ├── {commit_id}.json  # Commit 元数据（DAG 节点）
│       │   └── ...
│       └── trees/
│           ├── {tree_hash}/      # 完整目录树快照（内容寻址）
│           │   ├── content.json
│           │   ├── .abstract.md
│           │   └── .overview.md
│           └── ...
```

### 1.6 VersionStore trait（替代 v1 的 MVCC 方法）

```rust
// agent-context-db/src/version/store.rs

#[async_trait]
pub trait VersionStore: Send + Sync {
    // === 提交 ===
    async fn commit(&self, scope: &ContextUri, changes: ChangeSet, meta: CommitMeta)
        -> Result<CommitId, VersionError>;
    async fn commit_on(&self, branch: &BranchName, changes: ChangeSet, meta: CommitMeta)
        -> Result<CommitId, VersionError>;

    // === 分支 ===
    async fn create_branch(&self, scope: &ContextUri, name: BranchName, from: CommitId, bt: BranchType)
        -> Result<Branch, VersionError>;
    async fn list_branches(&self, scope: &ContextUri) -> Result<Vec<Branch>, VersionError>;
    async fn delete_branch(&self, scope: &ContextUri, name: &BranchName) -> Result<(), VersionError>;
    async fn switch_head(&self, scope: &ContextUri, branch: &BranchName) -> Result<(), VersionError>;

    // === 标签 ===
    async fn create_tag(&self, scope: &ContextUri, tag: Tag) -> Result<(), VersionError>;
    async fn move_tag(&self, scope: &ContextUri, name: &TagName, to: CommitId) -> Result<(), VersionError>;
    async fn list_tags(&self, scope: &ContextUri) -> Result<Vec<Tag>, VersionError>;
    async fn evaluate_semantic_tags(&self, scope: &ContextUri) -> Result<Vec<TagUpdate>, VersionError>;

    // === 读取 ===
    async fn log(&self, scope: &ContextUri, opts: &LogOpts) -> Result<Vec<Commit>, VersionError>;
    async fn show_commit(&self, commit: &CommitId) -> Result<CommitDetail, VersionError>;
    async fn read_at(&self, uri: &ContextUri, ref_: VersionRef, level: ContentLevel)
        -> Result<ContentPayload, VersionError>;

    // === 时间旅行 ===
    async fn asof_tree(&self, scope: &ContextUri, when: AsOfTime) -> Result<TreeSnapshot, VersionError>;
    async fn asof_read(&self, uri: &ContextUri, when: AsOfTime, level: ContentLevel)
        -> Result<ContentPayload, VersionError>;
    async fn timeline(&self, scope: &ContextUri, range: TimeRange) -> Result<Vec<TimelineEntry>, VersionError>;

    // === 快照 ===
    async fn snapshot(&self, scope: &ContextUri, name: &str) -> Result<SnapshotId, VersionError>;
    async fn restore_snapshot(&self, snapshot: &SnapshotId) -> Result<CommitId, VersionError>;
    async fn list_snapshots(&self, scope: &ContextUri) -> Result<Vec<SnapshotMeta>, VersionError>;

    // === 合并 ===
    async fn merge(&self, scope: &ContextUri, from: &BranchName, into: &BranchName, strategy: MergeStrategy)
        -> Result<MergeResult, VersionError>;
    async fn cherry_pick(&self, scope: &ContextUri, commit: &CommitId, onto: &BranchName)
        -> Result<CommitId, VersionError>;
    async fn rebase(&self, scope: &ContextUri, branch: &BranchName, onto: &BranchName)
        -> Result<Vec<CommitId>, VersionError>;

    // === Diff ===
    async fn diff_commits(&self, scope: &ContextUri, a: &CommitId, b: &CommitId)
        -> Result<TreeDiff, VersionError>;
    async fn diff_branches(&self, scope: &ContextUri, a: &BranchName, b: &BranchName)
        -> Result<TreeDiff, VersionError>;

    // === 因果链 ===
    async fn provenance(&self, uri: &ContextUri) -> Result<ProvenanceGraph, VersionError>;
    async fn impact_analysis(&self, commit: &CommitId) -> Result<ImpactGraph, VersionError>;

    // === 压缩与 GC ===
    async fn squash(&self, scope: &ContextUri, commits: Vec<CommitId>, message: &str)
        -> Result<CommitId, VersionError>;
    async fn gc(&self, scope: &ContextUri, policy: &GcPolicy) -> Result<GcReport, VersionError>;
}

/// 版本引用：可指向 commit/branch/tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionRef {
    Commit(CommitId),
    Branch(BranchName),
    Tag(TagName),
    Head,  // 当前 HEAD
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AsOfTime {
    Timestamp(chrono::DateTime<chrono::Utc>),
    Commit(CommitId),
    Tag(TagName),
    Relative(chrono::Duration),  // "3 days ago"
}

#[derive(Debug, Clone, Default)]
pub struct LogOpts {
    pub max_count: Option<usize>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub author: Option<String>,
    pub path: Option<ContextUri>,  // 只看某路径的变更
    pub grep: Option<String>,      // commit message 搜索
}
```

### 1.7 合并策略

```rust
// agent-context-db/src/version/merge.rs

#[derive(Debug, Clone)]
pub enum MergeStrategy {
    /// 三路合并：自动解决可合并部分，冲突保留
    ThreeWay { conflict_resolver: ConflictResolver },

    /// CRDT 自动合并：利用 uwu-crdt 的无冲突合并
    CrdtAuto { crdt_type: CrdtType },

    /// LLM 语义合并：冲突部分交由 LLM 决策
    LlmAssisted { llm: Arc<dyn LlmClient> },

    /// 强制覆盖：from 覆盖 into（需 Guard 放行）
    ForceOverwrite,

    /// 拒绝冲突：有冲突直接失败
    FailOnConflict,
}

#[derive(Debug, Clone)]
pub enum ConflictResolver {
    /// 按时间戳：后写赢
    LastWriteWins,
    /// 按优先级：指定分支优先
    BranchPriority { priority: Vec<BranchName> },
    /// 按作者角色：Orchestrator > Executor > Observer
    RolePriority,
    /// 人工决策：冲突写入 .conflict 文件待人工解决
    Manual,
    /// LLM 仲裁
    LlmArbitrate { llm: Arc<dyn LlmClient> },
}

pub struct MergeResult {
    pub merge_commit: Option<CommitId>,
    pub conflicts: Vec<MergeConflict>,
    pub auto_resolved: usize,
    pub manual_required: usize,
}

pub struct MergeConflict {
    pub uri: ContextUri,
    pub base: Option<ContentHash>,
    pub ours: ContentHash,
    pub theirs: ContentHash,
    pub conflict_type: ConflictType,
}

pub enum ConflictType {
    ContentDiverged,  // 双方都改了同一内容
    DeleteModify,     // 一方删一方改
    RenameConflict,   // 重命名冲突
    SemanticConflict, // 语义层面冲突（LLM 判定）
}
```

### 1.8 差异化版本策略

```rust
// agent-context-db/src/version/policy.rs

/// 按上下文类型应用不同版本策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionPolicy {
    pub scope: UriCategory,
    pub strategy: VersionStrategy,
    pub retention: RetentionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionStrategy {
    /// 不可变：只允许新增，不允许修改/删除（Events/Cases）
    Immutable,

    /// 线性 MVCC：单线版本链（Metacog 数据、日志类）
    LinearMvcc,

    /// 可分支：完整 DAG（Wiki/State/Resources）
    Branchable {
        max_branches: usize,
        auto_cleanup_forks: bool,  // fork 推演完自动删分支
    },

    /// 可合并：CRDT 优先 + DAG（Memory 分类中的可合并类型）
    Mergeable {
        crdt_type: CrdtType,
        auto_merge_on_write: bool,
    },

    /// 快照式：定期全量快照，不存增量（大块二进制资源）
    Snapshot { interval: chrono::Duration },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub max_versions: Option<usize>,
    pub max_age: Option<chrono::Duration>,
    pub gc_action: GcAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GcAction {
    Delete,           // 直接删除
    ArchiveToCold,    // 归档到冷存储
    Squash,           // 压缩为单个 commit
    Keep,             // 保留
}

/// 默认策略表
pub fn default_policies() -> Vec<VersionPolicy> {
    vec![
        VersionPolicy {
            scope: UriCategory::Sessions,
            strategy: VersionStrategy::Immutable,
            retention: RetentionPolicy {
                max_versions: None, max_age: Some(chrono::Duration::days(90)),
                gc_action: GcAction::ArchiveToCold,
            },
        },
        VersionPolicy {
            scope: UriCategory::State,
            strategy: VersionStrategy::Branchable {
                max_branches: 16, auto_cleanup_forks: true,
            },
            retention: RetentionPolicy {
                max_versions: Some(100), max_age: Some(chrono::Duration::days(30)),
                gc_action: GcAction::Squash,
            },
        },
        VersionPolicy {
            scope: UriCategory::Wiki,
            strategy: VersionStrategy::Branchable {
                max_branches: 32, auto_cleanup_forks: false,
            },
            retention: RetentionPolicy {
                max_versions: None, max_age: None, gc_action: GcAction::Keep,
            },
        },
        VersionPolicy {
            scope: UriCategory::Metacog,
            strategy: VersionStrategy::LinearMvcc,
            retention: RetentionPolicy {
                max_versions: Some(1000), max_age: Some(chrono::Duration::days(7)),
                gc_action: GcAction::Squash,
            },
        },
        // Memories: Profile/Preferences/Entities/Patterns/Tools/Skills → Mergeable
        // Memories: Events/Cases → Immutable
    ];
}
```

---

### 2.1 子树快照

```rust
// agent-context-db/src/version/snapshot.rs

/// 子树快照：冻结任意目录树在某个时间点的状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtreeSnapshot {
    pub id: SnapshotId,
    pub scope: ContextUri,          // 快照的根目录
    pub root_commit: CommitId,      // 对应的 commit
    pub tree_hash: ContentHash,     // 完整树哈希
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size_bytes: u64,
    pub entry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub Uuid);

/// 快照用途
pub enum SnapshotPurpose {
    /// 调试快照：Agent 每步推理前自动快照
    Debug { step: u64, session: Uuid },
    /// 审计快照：合规要求的时间点冻结
    Audit { period: String },
    /// 部署快照：配置变更前冻结
    PreChange { change_id: String },
    /// 用户手动
    Manual,
}
```

### 2.2 时间旅行查询

```rust
// agent-context-db/src/version/timetravel.rs

/// ASOF 查询：还原任意时间点的上下文状态
pub struct TimeTravel;

impl TimeTravel {
    /// 还原某个 URI 在指定时间点的内容
    pub async fn read_at(&self, uri: &ContextUri, when: AsOfTime, level: ContentLevel)
        -> Result<ContentPayload, VersionError>;

    /// 还原整个子树在指定时间点的状态
    pub async fn tree_at(&self, scope: &ContextUri, when: AsOfTime)
        -> Result<TreeSnapshot, VersionError>;

    /// 生成时间线：某 URI 的变更历史时间轴
    pub async fn timeline(&self, uri: &ContextUri, range: TimeRange)
        -> Result<Vec<TimelineEntry>, VersionError>;

    /// 差量回放：从时间点 A 到 B 之间的所有变更
    pub async fn replay(&self, scope: &ContextUri, from: AsOfTime, to: AsOfTime)
        -> Result<Vec<Commit>, VersionError>;

    /// 对比两个时间点的差异
    pub async fn diff_at(&self, scope: &ContextUri, a: AsOfTime, b: AsOfTime)
        -> Result<TreeDiff, VersionError>;
}

#[derive(Debug, Clone)]
pub struct TimelineEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub commit: CommitId,
    pub change: ChangeSet,
    pub author: Author,
    pub message: String,
}
```

### 2.3 State fork 与版本管理集成

```rust
// agent-context-db/src/uwu/state_bridge.rs (v2 升级)

impl StateBridge {
    /// fork 推演：创建命名分支
    pub async fn fork(&self, agent_id: &str, scope: StateScope, fork_name: &str)
        -> Result<ForkHandle, VersionError>
    {
        let scope_uri = state_dir_uri(agent_id, scope);
        let head = self.versions.read_head(&scope_uri).await?;
        let branch = self.versions.create_branch(
            &scope_uri,
            BranchName(fork_name.into()),
            head,
            BranchType::StateFork,
        ).await?;
        // 切换到 fork 分支
        self.versions.switch_head(&scope_uri, &branch.name).await?;
        Ok(ForkHandle { branch: branch.name, scope: scope_uri })
    }

    /// 推演完成：晋升（合并回 main）或丢弃（删除分支）
    pub async fn promote_fork(&self, fork: &ForkHandle, strategy: MergeStrategy)
        -> Result<CommitId, VersionError>
    {
        self.versions.merge(
            &fork.scope, &fork.branch, &BranchName("main".into()), strategy
        ).await
    }

    pub async fn discard_fork(&self, fork: &ForkHandle) -> Result<(), VersionError> {
        self.versions.switch_head(&fork.scope, &BranchName("main".into())).await?;
        self.versions.delete_branch(&fork.scope, &fork.branch).await
    }

    /// 多策略并行探索：同时 fork N 个分支推演不同策略
    pub async fn parallel_explore(&self, agent_id: &str, scope: StateScope, strategies: &[&str])
        -> Result<Vec<ForkHandle>, VersionError>
    {
        let mut forks = Vec::new();
        for s in strategies {
            forks.push(self.fork(agent_id, scope, &format!("explore-{}", s)).await?);
        }
        Ok(forks)
    }

    /// 选最优分支晋升（由 Metacognition.evaluate 决策）
    pub async fn promote_best(&self, forks: &[ForkHandle], scorer: impl ForkScorer)
        -> Result<CommitId, VersionError>
    {
        let best = scorer.pick_best(forks).await?;
        self.promote_fork(best, MergeStrategy::ForceOverwrite).await
    }
}

#[async_trait]
pub trait ForkScorer: Send + Sync {
    async fn score_fork(&self, fork: &ForkHandle) -> Result<f32, VersionError>;
    async fn pick_best<'a>(&self, forks: &'a [ForkHandle]) -> Result<&'a ForkHandle, VersionError>;
}
```

---

## 5. 破坏性重构清单

### 4.1 crate 删除/新建/重构清单

| crate | 操作 | 说明 | 破坏性 |
|---|---|---|---|
| **agent-wiki** | **删除** | 由 `uwu_wiki`（去存储层）替代；context-db-uwu 实现 `WikiStorage` trait 注入；agent-wiki API 调用方迁移到 `WikiSpace` | 高 |
| **agent-memory** | **重构为薄适配层** | UnifiedMemory trait 保留为兼容接口，实现委托 context-db；VectorStore trait 标记 `#[deprecated]` | 中 |
| **agent-context-db** | **新建（通用核心）** | L1-L7，持有 PG+Qdrant 双层；无 uwu 依赖，可独立发布 | - |
| **agent-context-db-uwu** | **新建（专有扩展）** | L8，五维 FS 映射 + Guard + Fork + MetacogBridge + CharacterConstraint；依赖 agent-context-db + 全部 uwu crate；**不再负责 WikiStorage 实现** | - |
| **agent-sidecar-consolidator** | **删除** | 重构为内嵌异步管线 SemanticQueue，Sidecar crate 消失 | 高 |
| **agent-sidecar-monitor** | **保留** | 监控职责不变，订阅 context-db 内嵌管线事件 | 无 |
| agent-session | **重构** | `Session.checkpoints` → `SessionCompressor.commit_phase1/2` | 中 |
| agent-state | **重构** | 内存数据结构保留，持久化层下沉 context-db-uwu | 低 |
| agent-persona | **重构** | 关系图持久化下沉 context-db-uwu，内存图保留 | 低 |
| agent-learning | **重构** | LearnNode 触发源改为 SemanticQueue 事件流 | 中 |
| agent-metacognition | **重构** | 校准数据从 context-db-uwu `metacog/` 目录读取 | 低 |
| agent-character | **保留** | 价值观作为 context-db-uwu 写入约束，自身不变 | 无 |
| agent-reaction | **保留** | 规则可从 context-db `experiences/` 学习（升级点） | 无 |
| uwu-crdt | **保留** | **合并计算层，不持有存储**：内存执行 CRDT 合并算子，合并后状态和 Op 日志均持久化到 PG；同时作为 uwu_wiki wiki-collab 的 CRDT 后端 | 无 |
| agent-guard | **保留** | egress 闸门拦截 context-db-uwu 写入 | 无 |
| uwu_database | **保留但解耦** | context-db 不依赖它，直接持有 PG+Qdrant | 低 |
| **uwu_wiki** | **新增依赖** | wiki-core + wiki-llm（workflow）作为通用核心的 L7 wiki 层基础；去存储层后由 context-db-uwu 注入 WikiStorage 实现 | - |

### 4.2 依赖图变化前后对比

**重构前**（17+ crate）：

```
agent-core
  ├── agent-session
  │     ├── agent-state (→ uwu_database)
  │     ├── agent-persona (→ uwu_database)
  │     ├── agent-metacognition
  │     ├── agent-character
  │     ├── agent-reaction
  │     ├── agent-memory (→ uwu_database, Qdrant)
  │     └── agent-wiki (→ uwu_database)
  ├── agent-task
  ├── agent-collaboration
  ├── agent-perception
  ├── agent-reasoning
  ├── agent-execution
  ├── agent-guard
  ├── agent-learning (→ agent-sidecar-consolidator)
  ├── agent-uncertainty
  ├── uwu-crdt
  ├── agent-tools
  └── agent-mesh
agent-sidecar-consolidator (独立进程)
agent-sidecar-monitor (独立进程)
```

**重构后**（净减少 1 crate：删 wiki + consolidator，新增 context-db + context-db-uwu；uwu_wiki 作为新依赖引入）：

```
agent-core
  ├── agent-session
  │     ├── agent-state (→ 持久化委托 agent-context-db-uwu)
  │     ├── agent-persona (→ 持久化委托 agent-context-db-uwu)
  │     ├── agent-metacognition (→ 读 context-db-uwu metacog/)
  │     ├── agent-character (→ 写约束到 context-db-uwu)
  │     ├── agent-reaction
  │     ├── agent-memory (薄适配层,实现委托 context-db)
  │     └── ★ agent-context-db-uwu (专有扩展 L8)
  │           └── ★ agent-context-db (通用核心 L1-L7，持有 PG+Qdrant)
  │                 └── ★ uwu_wiki (wiki 子域；WikiStorage 由 context-db-uwu 注入)
  ├── agent-task
  ├── agent-collaboration
  ├── agent-perception
  ├── agent-reasoning
  ├── agent-execution
  ├── agent-guard (egress 拦截 context-db-uwu 写入)
  ├── agent-learning (订阅 context-db SemanticQueue 事件)
  ├── agent-uncertainty
  ├── uwu-crdt (合并计算层，不持久化；PG 是唯一存储后端)
  ├── agent-tools
  └── agent-mesh
agent-sidecar-monitor (保留,订阅内嵌管线事件)
[删除] agent-sidecar-consolidator
[删除] agent-wiki
```

### 4.3 uwu_wiki 集成方案（wiki 子域）

`uwu_wiki` 采用存储层外部注入设计（v2.1），`agent-context-db` 实现 `WikiStorage` trait 后注入。

```rust
// agent-context-db/src/wiki/wiki_storage.rs
// ContextDbWikiStorage 在通用核心 L7 wiki 层实现，复用 L7 storage 已持有的 PG/Qdrant 连接

use uwu_wiki::WikiStorage;

/// context-db 实现的 WikiStorage，对接 PG+Qdrant 双层存储
pub struct ContextDbWikiStorage {
    pg: Arc<PgPool>,
    qdrant: Arc<QdrantClient>,
}

#[async_trait]
impl WikiStorage for ContextDbWikiStorage {
    fn vector_store(&self) -> Arc<dyn VectorStore> {
        Arc::new(QdrantWikiVectorStore::new(self.qdrant.clone()))
    }
    fn doc_store(&self) -> Arc<dyn DocStore> {
        Arc::new(PgDocStore::new(self.pg.clone()))
    }
    fn op_log(&self) -> Arc<dyn OpLog> {
        Arc::new(PgOpLog::new(self.pg.clone()))
    }
}

// 初始化入口
pub fn init_wiki_space(pg: Arc<PgPool>, qdrant: Arc<QdrantClient>) -> WikiSpace {
    let storage = Arc::new(ContextDbWikiStorage { pg, qdrant });
    WikiSpace::new(SpaceId::default(), storage)
}
```

**URI 映射**：

```
uwu://agent-{id}/wiki/{space}/{doc_id}           → WikiSpace::get_doc(doc_id)
uwu://agent-{id}/wiki/{space}/{doc_id}/{block_id} → WikiSpace::get_block(doc_id, block_id)
uwu://agent-{id}/wiki/{space}/index              → wiki 目录页（IngestPipeline 维护）
```

**LLM Wiki 工作流接入**：

```rust
// 由 context-db service 层统一暴露工作流接口
pub struct WikiDomain {
    space: WikiSpace,
    ingest: IngestPipeline,
    query: QueryPipeline,
    linter: WikiLinter,
}

impl WikiDomain {
    /// 消化新原料进 wiki
    pub async fn ingest(&self, source: IngestSource) -> Result<IngestResult>;
    /// 问答（含可选反写）
    pub async fn query(&self, question: &str, policy: WriteBackPolicy) -> Result<QueryResult>;
    /// 触发审计
    pub async fn lint(&self) -> Result<LintReport>;
}
```

**事件桥接**（wiki 事件 → agent-mesh）：

| uwu_wiki 事件 | agent-mesh 主题 | 说明 |
|---|---|---|
| `wiki.ingest.completed` | `context.wiki.ingest.done` | Ingest 完成通知 |
| `wiki.ingest.contradiction` | `context.wiki.contradiction` | 发现矛盾，LearnNode 可处理 |
| `wiki.query.write_back` | `context.wiki.write_back` | 答案反写触发 |
| `wiki.lint.completed` | `context.wiki.lint.done` | 携带 LintReport |

### 4.4 agent-memory 重构方案

```rust
// agent-memory/src/lib.rs (重构后薄适配层)

#[deprecated(note = "use agent_context_db::ContextStore directly")]
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn search(&self, query: &Embedding, opts: &SearchOpts) -> Vec<Memory>;
    async fn upsert(&self, id: MemoryId, embedding: Embedding, metadata: &Value);
}

/// 兼容旧 API，实现委托 context-db
pub struct UnifiedMemory {
    ctx: Arc<dyn agent_context_db::ContextStore>,
    retriever: Arc<dyn agent_context_db::HierarchicalRetriever>,
}

impl UnifiedMemory {
    pub async fn retrieve(&self, query: &str) -> Vec<Memory> {
        self.retriever.retrieve(query, &Default::default()).await
            .map(|r| r.hits.into_iter().map(hit_to_memory).collect())
            .unwrap_or_default()
    }
    pub async fn retrieve_typed(&self, query: &str, class: MemoryType) -> Vec<Memory> { /* ... */ }
    pub async fn persist_state(&self, snap: &StateSnapshot) { /* 写 state/ 目录 */ }
    pub async fn persist_persona(&self, p: &Persona) { /* 写 persona/ 目录 */ }
    pub async fn consolidate(&self, episode: &Episode) { /* 触发 commit_phase2 */ }
}
```

### 4.4 agent-sidecar-consolidator 重构方案

**原职责**：独立进程消费 wiki/memory 事件 → LearnNode + Guard 博弈 → 持久化

**重构后**：内嵌为 `agent-context-db::compressor::SemanticQueue` + `agent-learning` 的内嵌订阅者。

**LearnNode + Guard 博弈职责去向**：

| 原职责 | 重构后去向 | 同步/异步 |
|---|---|---|
| LearnNode 触发 | SemanticQueue `subscribe_done()` 通道推送 `agent-learning` | 异步 |
| Guard egress 博弈 | `agent-guard` 在 context-db 写入路径同步拦截 | 同步闸门 |
| SkillVersion 沙箱验证 | 保留 `agent-learning`，沙箱结果回写 `skills/{name}/versions/` | 异步 |
| 记忆巩固 | SemanticQueue worker 内的 MemoryExtractor | 异步 |

**Sidecar crate 命运**：`agent-sidecar-consolidator` crate **删除**。NATS 订阅逻辑改为 SemanticQueue 的 in-process tokio task。

---

## 6. 异步语义处理管线设计

### 5.1 管线整体架构

```
┌──────────────────────────────────────────────────────────────┐
│ agent-session::process_turn                                    │
│   ... 主循环结束 ...                                            │
│         │                                                      │
│         ▼                                                      │
│   SessionCompressor.commit_phase1 (同步)                       │
│     1. compression_index++                                     │
│     2. 写 messages.jsonl → uwu://.../sessions/{id}/archive/{N}/│
│     3. 清空当前消息                                             │
│     4. 返回 task_id                                            │
│         │                                                      │
│         ▼                                                      │
│   SemanticQueue.enqueue(SemanticTask::ExtractMemories)        │
│         │                                                      │
│         ▼                                                      │
└────── 内嵌 tokio task (替换 Sidecar 进程) ──────────────────────┘
          │
          ▼
┌──────────────────────────────────────────────────────────────┐
│ Worker (tokio mpsc 通道驱动,不走 NATS)                          │
│                                                                │
│  阶段1: MemoryExtractor.extract(archive)                       │
│    → 提取候选记忆 (8 种分类)                                     │
│                                                                │
│  阶段2: MemoryExtractor.deduplicate(candidates)               │
│    → LLM 决策 skip/create/merge/delete                          │
│    → 应用变更到 context-db memories/                            │
│                                                                │
│  阶段3: SemanticProcessor.generate_abstract/overview           │
│    → 为新增/更新条目生成 L0/L1                                   │
│                                                                │
│  阶段4: SemanticProcessor.aggregate_upward(parent)             │
│    → 子目录 L0 聚合到父目录 L1                                   │
│                                                                │
│  阶段5: TrajectoryExtractor.extract_trajectory(archive)        │
│    → 写 trajectories/{tid}/                                     │
│    → 触发 InduceExperience (多条相关轨迹归纳)                    │
│                                                                │
│  阶段6: 写 memory_diff.json (审计)                              │
│  阶段7: 写 .done 标记                                           │
│                                                                │
│  阶段8: SemanticQueue.publish(TaskDoneEvent)                    │
│    → 推送给 agent-learning (LearnNode 触发)                     │
│    → 推送给 agent-metacognition (校准数据更新)                   │
└──────────────────────────────────────────────────────────────┘
```

### 5.2 与 agent-mesh 事件网格的关系

**关键决策**：内嵌语义管线 **不走 NATS**，使用 **tokio mpsc 通道**。

| 通道类型 | 用途 | 场景 |
|---|---|---|
| tokio mpsc | commit_phase2 worker → agent-learning / agent-metacognition | 单进程内高频，零序列化开销 |
| NATS JetStream | 跨 Agent 协作、Sidecar-Monitor 订阅、多 Agent wiki CRDT Op 广播 | 跨进程，需持久化/重放 |

**事件流架构**：

```
commit_phase2 worker
  │
  ├─ tokio::mpsc ─→ agent-learning::LearnNodeTrigger (in-process)
  ├─ tokio::mpsc ─→ agent-metacognition::CalibrationFeed (in-process)
  └─ agent-mesh.publish("uwu.context.committed", payload)
       └─→ NATS ─→ agent-sidecar-monitor (跨进程,监控)
```

**理由**：
1. 语义处理是单进程内高频任务，NATS 序列化开销不必要
2. Sidecar 模式的崩溃隔离价值在 uwu MVCC + checkpoint 已有等价保障
3. tokio mpsc 提供 backpressure，SemanticQueue 满载时 commit_phase1 同步阻塞，自然限流

### 5.3 LearnNode + Guard 博弈融入新管线

原 Consolidator 流程：
```
事件 → Consolidator → LearnNode.check(condition) → Guard.check_egress → 持久化
```

新管线流程：
```
SemanticQueue.worker
  ├─ 提取记忆候选
  ├─ 对每个候选:
  │    ├─ Guard.check_egress(candidate) ── 同步闸门 (不再异步博弈)
  │    │     ├─ Pass → ContextStore.write
  │    │     └─ Deny → 标记 pending_human_approval,写 .pending 文件
  │    └─ ContextStore.write 成功 → publish MemoryWritten event
  └─ TaskDoneEvent → agent-learning::LearnNode.on_memory_written
       └─ 触发条件评估 → 可能产生新 SkillVersion 沙箱任务
```

**Guard 博弈简化**：原"异步博弈"模型改为"**同步闸门 + 异步反馈**"。写入路径上 GuardLayer.check_egress 同步拦截（不可绕过），LearnNode 触发条件评估异步进行（不阻塞 commit_phase2）。

---

## 7. 与五维的深度融合

### 6.1 State（短/中/长程 WS + MVCC）作为一等公民

```rust
// agent-context-db/src/uwu/state_bridge.rs

pub struct StateBridge {
    store: Arc<dyn ContextStore>,
    retriever: Arc<dyn HierarchicalRetriever>,
}

impl StateBridge {
    /// 从 FS 加载 State 快照（按 scope）
    pub async fn load(&self, agent_id: &str, scope: StateScope, version: Option<MvccVersion>)
        -> Result<StateSnapshot, ContextError>
    {
        let dir = state_dir_uri(agent_id, scope);
        // 1. 读 .overview.md (L1) 快速判断版本
        // 2. 读 content.json (L2) 反序列化
        // 3. version 指定时从 versions/v{N}/ 读
    }

    /// 写入新 State 快照（MVCC 新版本）
    pub async fn checkpoint(&self, agent_id: &str, scope: StateScope, snap: &StateSnapshot)
        -> Result<MvccVersion, ContextError>
    {
        let uri = state_snapshot_uri(agent_id, scope, snap.id());
        let entry = ContextEntry {
            uri: uri.clone(),
            l0_abstract: summarize_state(snap),
            l1_overview: Some(state_navigation(snap)),
            l2_detail_uri: Some(self.store_content(snap).await?),
            content_type: ContentType::Text,
            metadata: ContextMeta { state_scope: Some(scope), .. },
            mvcc_version: self.store.version_history(&uri).await?.len() as u64 + 1,
            ..
        };
        self.store.write(entry).await
    }

    /// Fork 推演沙盒：复制当前 State 到 fork 目录，推演完成后回滚或晋升
    pub async fn fork(&self, agent_id: &str, scope: StateScope) -> Result<ForkHandle, ContextError> {
        // 1. 读当前版本
        // 2. 复制到 uwu://.../state/{scope}/_forks/{fork_id}/
        // 3. 返回 ForkHandle，推演期间写入 fork 目录
        // 4. 晋升:fork → 主版本；回滚:删除 fork 目录
    }
}
```

### 6.2 Persona（关系图）存入 FS 并支持图谱查询

```rust
// agent-context-db/src/uwu/persona_bridge.rs

impl PersonaBridge {
    pub async fn upsert_relation(&self, agent_id: &str, target: &str, rel: &Relation)
        -> Result<(), ContextError>
    {
        // 1. 写 relation.md (L2 完整)
        // 2. 异步触发 .abstract.md / .overview.md 重生成
        // 3. 异步触发 .graph.overview.md 邻接表更新
    }

    /// 图谱查询：K 跳邻居
    pub async fn query_neighbors(&self, agent_id: &str, k: usize)
        -> Result<Vec<RelationNode>, ContextError>
    {
        // 1. 读 .graph.overview.md (L1 邻接表)
        // 2. K=1 直接返回；K>1 递归读邻居的 .overview.md
    }

    /// 关系推理：基于历史轨迹预测关系变化
    pub async fn predict_relation_drift(&self, agent_id: &str, target: &str)
        -> Result<RelationDrift, ContextError>
    {
        // 调 HierarchicalRetriever.retrieve_typed(PersonaRelation)
        // 结合 metacog/ pred_errors 历史预测
    }
}
```

### 6.3 Metacognition 校准数据检索

> **真值源边界（对接 agent-metacognition 现有实现）**
> `pred_error` 有两个已落地的载体,本设计不另造范式,而是显式对齐:
> - **派生层** `AgentState.long_term.accumulated_pred_error: f32` — sample 序列的 EMA 投影,meta_score 热路径直接读它(零 IO),可随时从事实层重算,不是独立真值。
> - **事实层** = `CalibrationHistory`(内存环形缓冲,cap 1000,热)+ `metacog/pred_errors/`(evict 后冷归档),两者是同一序列的连续体。
> - 类型对齐:`PredErrorSample ≡ agent-metacognition 的 CalibrationRecord`(`predicted_state_id` / `actual_state_id` / `calibration` / `meta_score` / `ts`),无需新造类型。
> `metacog/pred_errors/` 因此不是"原始事实流",而是环形缓冲 evict 后的只读归档 + 派生标量的重算来源。检索校准历史时需合并内存(热,未 evict)与 FS(冷,已归档)两处。

```rust
// agent-context-db/src/uwu/metacog_bridge.rs

impl MetacogBridge {
    /// 归档:CalibrationHistory evict 出的 CalibrationRecord 落盘为冷存
    /// (热路径写的是内存环形缓冲,不是这里;此方法仅在 evict 时触发)
    pub async fn log_pred_error(&self, agent_id: &str, sample: &PredErrorSample)
        -> Result<(), ContextError>
    {
        let uri = format!("uwu://.../agent/{}/metacog/pred_errors/{}.json", agent_id, sample.ts);
        self.store.write(ContextEntry { /* L2 JSON */ }).await
    }

    /// 检索历史校准数据（用于三信号自校准）
    pub async fn retrieve_calibration(&self, agent_id: &str, window: TimeWindow)
        -> Result<Vec<PredErrorSample>, ContextError>
    {
        // 1. 先取内存 CalibrationHistory 中落在窗口内的记录(热,未 evict)
        // 2. ls metacog/pred_errors/ 取窗口内已归档文件(冷),并行 read(L2) 反序列化
        // 3. 合并去重(按 ts),热数据优先
        // 4. 可选：retrieve_typed 在 experiences/ 中找相似场景校准历史
    }
}
```

### 6.4 Character 核心价值观作为写入约束

```rust
// agent-context-db/src/uwu/character_constraint.rs

pub struct CharacterConstraint {
    core_values: Vec<CoreValue>, // 从 uwu://.../character/core_values.md 加载
}

impl CharacterConstraint {
    /// ContextStore.write 前置钩子
    pub fn check_write(&self, entry: &ContextEntry) -> Result<(), ConstraintViolation> {
        // 1. 解析 entry 内容
        // 2. 对每条 core_value 调用 value.check(&content)
        // 3. 违反 → 返回 ConstraintViolation
    }

    /// 对 wiki 协作编辑 CRDT 操作的约束
    pub fn check_crdt_op(&self, op: &CrdtOp) -> Result<(), ConstraintViolation> { /* ... */ }
}
```

### 6.5 Reaction 规则从经验记忆自动学习（升级点）

```rust
// agent-context-db/src/uwu/reaction_learner.rs

pub struct ReactionLearner {
    retriever: Arc<dyn HierarchicalRetriever>,
    reaction_engine: Arc<dyn ReactionEngine>,
}

impl ReactionLearner {
    /// 周期性扫描 experiences/，寻找高频 Situation 模式
    pub async fn induce_rules(&self, agent_id: &str) -> Result<Vec<NewRule>, ContextError> {
        // 1. ls experiences/ 取最近 N 条
        // 2. 聚类 Situation 字段
        // 3. 高频模式 → 候选规则
        // 4. 经 Guard.check_egress + 沙箱验证后注册到 agent-reaction
    }
}
```

---

### 6.1 State：多策略并行探索

```rust
// v1: fork() → 推演 → promote/discard（单分支）
// v2: parallel_explore() → 多分支并行 → promote_best（多策略并行）

impl StateBridge {
    /// ToT（Tree of Thought）集成：每个思维分支 = 一个 State fork 分支
    pub async fn tot_explore(&self, agent_id: &str, scope: StateScope, thoughts: &[Thought])
        -> Result<Vec<ForkHandle>, VersionError>
    {
        let mut forks = Vec::new();
        for (i, thought) in thoughts.iter().enumerate() {
            let fork = self.fork(agent_id, scope, &format!("tot-{}", i)).await?;
            // 在 fork 分支上应用 thought
            self.apply_thought(&fork, thought).await?;
            forks.push(fork);
        }
        Ok(forks)
    }

    /// Metacognition 评估各 fork 的 StateScore → promote_best
    pub async fn tot_select_and_promote(&self, forks: Vec<ForkHandle>, meta: &Metacognition)
        -> Result<CommitId, VersionError>
    {
        let scorer = TotForkScorer { meta: meta.clone() };
        self.promote_best(&forkes, scorer).await
    }
}
```

### 6.2 Metacognition：版本感知校准

```rust
impl MetacogBridge {
    /// 对比两个版本的预测误差（用于 fork 评估）
    pub async fn compare_fork_pred_error(&self, fork: &ForkHandle, baseline: &ForkHandle)
        -> Result<f32, VersionError>
    {
        let fork_pe = self.read_pred_error_at(&fork.branch).await?;
        let base_pe = self.read_pred_error_at(&baseline.branch).await?;
        Ok(fork_pe - base_pe)  // 负值 = fork 更优
    }

    /// 检索历史校准数据（支持版本引用）
    pub async fn retrieve_calibration_at(&self, agent_id: &str, ref_: VersionRef, window: TimeWindow)
        -> Result<Vec<PredErrorSample>, VersionError>
    {
        // 按版本引用读取历史 metacog 数据
    }
}
```

### 6.3 Persona：关系图版本演化

```rust
impl PersonaBridge {
    /// 关系图变更产生 commit，支持回溯"某时间点的关系状态"
    pub async fn relations_at(&self, agent_id: &str, when: AsOfTime)
        -> Result<RelationGraph, VersionError>;

    /// 关系演化时间线
    pub async fn relation_timeline(&self, agent_id: &str, target: &str, range: TimeRange)
        -> Result<Vec<RelationChange>, VersionError>;
}
```

### 6.4 Reaction：从历史版本学习规则

```rust
impl ReactionLearner {
    /// v2: 从指定版本范围的经验中归纳规则
    pub async fn induce_rules_from_range(&self, agent_id: &str, range: TimeRange)
        -> Result<Vec<NewRule>, VersionError>
    {
        // 1. asof_tree 读取 range 内的 experiences/
        // 2. 聚类 Situation 模式
        // 3. 高频模式 → 候选规则
        // 4. 经 Guard + 沙箱验证后注册
    }
}
```

---

## 8. 检索机制设计

### 7.1 HierarchicalRetriever 完整实现骨架

```rust
// agent-context-db/src/retrieve/retriever.rs

pub struct HierarchicalRetrieverImpl {
    store: Arc<dyn ContextStore>,
    vector_index: Arc<dyn VectorIndex>,
    intent_analyzer: Arc<dyn IntentAnalyzer>,
    reranker: Arc<dyn Reranker>,
    metacog: Option<Arc<dyn MetacogBridge>>, // 升级点：检索结果可被校准
}

#[async_trait]
impl HierarchicalRetriever for HierarchicalRetrieverImpl {
    async fn retrieve(&self, query: &str, ctx: &RetrieveContext)
        -> Result<RetrievalResult, ContextError>
    {
        let mut trace = RetrievalTrace::default();

        // 阶段1: 意图分析（生成 0-5 个类型化查询）
        let typed_queries = self.intent_analyzer.analyze(query, ctx).await?;
        trace.steps.push(TraceStep::IntentAnalysis {
            raw: query.to_string(), typed: typed_queries.clone(),
        });

        let mut all_hits = Vec::new();
        for tq in &typed_queries {
            // 阶段2: 初始定位（向量检索定位高分目录）
            let top_dirs = self.locate_dirs(tq, ctx).await?;
            trace.steps.push(TraceStep::InitialLocate {
                query: tq.clone(), top_dirs: top_dirs.clone(),
            });

            // 阶段3: 精细探索（目录内二次检索）
            for (dir, _) in &top_dirs {
                let candidates = self.intra_dir_search(dir, tq, ctx).await?;
                trace.steps.push(TraceStep::IntraDirSearch {
                    dir: dir.clone(), candidates: candidates.clone(),
                });

                // 阶段4: 递归深入（子目录逐层）
                for cand in candidates {
                    if let Some(deeper) = self.recursive_descent(&cand, tq, ctx, &mut trace).await? {
                        all_hits.extend(deeper);
                    }
                }
            }
        }

        // 阶段5: Rerank 精排
        let before = all_hits.len();
        let reranked = self.reranker.rerank(query, all_hits).await?;
        trace.steps.push(TraceStep::Rerank {
            input: before, kept: reranked.len(), model: "cross-encoder".into(),
        });

        // 升级点: Metacognition 校准
        let final_hits = if let Some(metacog) = &self.metacog {
            metacog.adjust_by_pred_error(&reranked).await.unwrap_or(reranked)
        } else { reranked };

        // 阶段6: 按预算加载内容（默认 L1，超预算降级 L0）
        let (hits, tokens) = self.load_within_budget(final_hits, ctx).await?;
        for h in &hits {
            trace.steps.push(TraceStep::Load {
                uri: h.uri.clone(), level: h.level, tokens: 0,
            });
        }

        Ok(RetrievalResult { hits, trace, tokens_used: tokens, intent: typed_queries })
    }
}
```

### 7.2 IntentAnalyzer 实现策略

```rust
// agent-context-db/src/retrieve/intent.rs

pub struct LlmIntentAnalyzer {
    llm: Arc<dyn LlmClient>,  // 走 LlmClient trait（MCP 或直连）
}

#[async_trait]
impl IntentAnalyzer for LlmIntentAnalyzer {
    async fn analyze(&self, query: &str, ctx: &RetrieveContext)
        -> Result<Vec<TypedQuery>, ContextError>
    {
        // Prompt: "将以下查询拆为 0-5 个类型化查询"
        // 输出 JSON: [{kind, text, target_dirs, expected_class}]
        // target_dirs 基于 ctx.user_id/agent_id 推测
        let typed: Vec<TypedQuery> = self.llm.complete_json(
            &build_intent_prompt(query, ctx),
            &TYPED_QUERY_SCHEMA,
            &LlmOpts { max_tokens: Some(1000), temperature: Some(0.0), .. },
        ).await?;
        Ok(typed)
    }
}

/// 备用：规则 based IntentAnalyzer（无 LLM 场景）
pub struct RuleBasedIntentAnalyzer { /* 关键词匹配 */ }
```

### 7.3 与 uwu 现有 retrieve/retrieve_typed 的关系

| uwu 原 API | 新 API | 关系 |
|---|---|---|
| `UnifiedMemory::retrieve(query)` | `HierarchicalRetriever::retrieve(query, ctx)` | 旧 API 委托新 API，默认 prefer_level=L1 |
| `UnifiedMemory::retrieve_typed(query, MemoryType)` | `HierarchicalRetriever::retrieve_typed(query, MemoryClass, ctx)` | MemoryType → MemoryClass 枚举映射 |
| `UnifiedMemory::consolidate(episode)` | `SessionCompressor::commit_phase2(task_id)` | consolidate 拆为两阶段 |

### 7.4 检索轨迹可视化与 tracing/OpenTelemetry 集成

```rust
// agent-context-db/src/retrieve/trace.rs

impl RetrievalTrace {
    /// 导出为 tracing spans
    pub fn to_spans(&self) -> Vec<tracing::Span> {
        self.steps.iter().map(|s| match s {
            TraceStep::IntentAnalysis { .. } => tracing::info_span!("intent_analysis"),
            TraceStep::InitialLocate { .. } => tracing::info_span!("initial_locate"),
            TraceStep::IntraDirSearch { .. } => tracing::debug_span!("intra_dir_search"),
            TraceStep::RecursiveDescent { .. } => tracing::debug_span!("recursive_descent"),
            TraceStep::Rerank { .. } => tracing::info_span!("rerank"),
            TraceStep::Load { .. } => tracing::trace_span!("load"),
        }).collect()
    }

    /// 导出为 OpenTelemetry attributes
    pub fn to_otel_attributes(&self) -> Vec<opentelemetry::KeyValue> {
        // 完整路径作为 retrieval.trace.uri_chain 属性
    }

    /// 导出为可视化 JSON（调试用）
    pub fn to_json(&self) -> serde_json::Value { /* ... */ }
}
```

每次 retrieve 默认开启 trace（`ctx.trace_enabled=true`），通过 tracing::instrument 自动埋点，SemanticQueue 完成时附带 trace_id 便于关联。

---


### 升级点设计（超越 OpenViking）

| # | 升级点 | OpenViking | uwu context-db | 实现 |
|---|---|---|---|---|
| U1 | **全 Rust 性能** | Python+Rust 混合 | 全 Rust，无 GIL，无序列化跨语言开销 | 原生实现 |
| U2 | **Metacog 校准检索** | 无 | 检索结果经 pred_error 校准，过滤历史预测失败的模式 | `HierarchicalRetrieverImpl.metacog` |
| U3 | **Guard 写入闸门** | 无 | 写入 context-db 经 GuardLayer egress 同步拦截，LearnNode 异步反馈 | `CharacterConstraint` + Guard 集成 |
| U4 | **CRDT 协作编辑** | 无 | wiki 子域通过 `uwu_wiki` 的 `wiki-collab` 模块实现多 Agent CRDT 无冲突合并；`uwu-crdt` 作为 wiki-collab 的 CRDT 后端 | `uwu_wiki::wiki-collab` + `uwu-crdt` |
| U5 | **State fork 推演** | 无 | 检索结果可喂入 fork 沙盒推演，推演完成回写 FS | `StateBridge.fork()` |
| U6 | **多租户** | 单租户文件操作 | account/user/agent 三级隔离 + uwu_auth | `TenantId` 全程透传 |
| U7 | **五维 FS 同构** | 无 State/Persona 概念 | State/Persona/Metacog/Character/Reaction 皆为 FS 一等目录 | 第 3 节目录结构 |
| U8 | **MVCC + L0/L1/L2 叠加** | 单写者 | MVCC 版本历史 + 每版本独立 L0/L1 | `versions/v{N}/` + 每版本 abstract |
| U9 | **检索轨迹 + tracing 融合** | 独立可视化 | 直接映射 tracing spans + OTel attributes | 第 7.4 节 |
| U10 | **Trajectory → Reaction 自动学习** | 无 | 经验记忆归纳新 Reaction 规则，经沙箱验证注册 | `ReactionLearner` |
| U11 | **事件网格协同** | HTTP API | 内嵌 tokio mpsc + NATS 双通道（高频内嵌，跨进程 NATS） | 第 5.2 节 |
| U12 | **WASM 沙箱执行记忆衍生计算** | 无 | 在记忆上跑 WASM 衍生计算（如统计、聚类） | 集成 uwu_wasm |
| U13 | **LLM 调用统一抽象** | 内置多 Provider 配置 | `LlmClient` trait，MCP 默认 + 直连 SDK 备选 | 第 2.1 节 |

---


## 9. 创新功能（15 项）

> **能力池,非首版范围**。下表按落地价值/成本给这 15 项定级,只有 Core 级进入 M0-M3,其余归 M4+ 或研究性储备。定级依据:是否单 Agent 场景必需、是否依赖尚未验证的模型(JEPA/因果/多模态)、失败是否可降级。

| 功能 | 定级 | 归属 | 理由 |
|---|---|---|---|
| F16 预测性预加载 | Research | M4+ | 依赖 JEPA 预测模型,未落地;失败仅损失性能,可后加 |
| F17 压缩感知加载 | Ext | M3 | L0/L1/L2 已有,压缩感知是其上的调度优化 |
| F18 跨 Agent 联邦 | Research | M4+ | 多 Agent 场景,首版单 Agent 不需要 |
| F19 知识晶体蒸馏 | Ext | M4+ | 依赖大量历史数据积累后才有意义 |
| F20 幻觉检测 | Core | M1 | 检索质量闸门,单 Agent 即需要,可用规则起步 |
| F21 自修复 | Ext | M4+ | 依赖 F5 provenance,复杂度高 |
| F22 遗忘曲线 | Ext | M3 | TTL/生命周期的策略层,M3 的 lifecycle 可承载 |
| F23 梦境巩固 | Research | M4+ | 探索性,收益未验证 |
| F24 版本差异推理 | Ext | M3 | 建立在 M2 版本系统上 |
| F25 安全沙箱 | Core | M3 | Guard 写约束的一部分,uwu 扩展必需 |
| F26 上下文经济模型 | Research | M4+ | token 预算已在 metacognition,重复 |
| F27 因果推断 | Research | M4+ | 依赖因果模型,未落地 |
| F28 增量检索学习 | Ext | M4+ | 检索器在线学习,M1 稳定后 |
| F29 多模态对齐 | Research | M4+ | 依赖多模态模型 |
| F30 时态推理 | Ext | M3 | 建立在 M2 时间旅行上 |

> Core=2 项进 M1-M3 主线;Ext=6 项作为对应里程碑的可选增强;Research=7 项进独立探索分支,不阻塞主线。一次性实现全部 15 项是本设计最大的过度设计风险。

### 1.1 F16 上下文预测性预加载（Predictive Prefetch）

> 基于 JEPA 预测模型，在 Agent 当前步推理时就预测下一步可能需要的上下文，异步预加载到热缓存。

```rust
// agent-context-db/src/uwu/prefetch.rs

/// 预测性预加载器
pub struct PredictivePrefetcher {
    store: Arc<dyn ContextStore>,
    cache: Arc<HotCache>,
    jepa_model: Arc<dyn JepaPredictor>,  // 复用 agent-state 的 JEPA
    prefetch_queue: tokio::sync::mpsc::Sender<PrefetchTask>,
}

#[derive(Debug, Clone)]
pub struct PrefetchTask {
    pub predicted_uris: Vec<ContextUri>,
    pub confidence: f32,
    pub prefetch_level: ContentLevel,  // 高置信度→L2，低→L0
    pub deadline: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait JepaPredictor: Send + Sync {
    /// 基于当前 State 预测下一步可能访问的上下文 URI
    async fn predict_next_contexts(
        &self, current_state: &AgentState, history: &[ContextAccessRecord]
    ) -> Vec<Prediction>;
}

#[derive(Debug, Clone)]
pub struct Prediction {
    pub uri: ContextUri,
    pub probability: f32,
    pub access_pattern: AccessPattern,  // Sequential/Random/Scan
}

impl PredictivePrefetcher {
    /// Agent 每步推理前调用
    pub async fn on_step_begin(&self, state: &AgentState) {
        let predictions = self.jepa_model.predict_next_contexts(state, &self.recent_accesses()).await;
        let high_conf: Vec<_> = predictions.iter().filter(|p| p.probability > 0.7).collect();
        let low_conf: Vec<_> = predictions.iter().filter(|p| p.probability > 0.3).collect();

        // 高置信度:预加载 L1 到热缓存
        for p in &high_conf {
            let _ = self.prefetch_queue.send(PrefetchTask {
                predicted_uris: vec![p.uri.clone()],
                confidence: p.probability,
                prefetch_level: ContentLevel::L1,
                deadline: Utc::now() + chrono::Duration::milliseconds(500),
            }).await;
        }
        // 低置信度:只预加载 L0（极低成本）
        for p in &low_conf {
            let _ = self.prefetch_queue.send(PrefetchTask {
                predicted_uris: vec![p.uri.clone()],
                confidence: p.probability,
                prefetch_level: ContentLevel::L0,
                deadline: Utc::now() + chrono::Duration::seconds(2),
            }).await;
        }
    }

    /// 命中率统计（喂回 Metacognition 校准 JEPA）
    pub async fn hit_rate(&self) -> PrefetchStats { /* ... */ }
}
```

**预期收益**：检索延迟 P99 降 40-60%（热缓存命中）。

### 1.2 F17 上下文压缩感知（Compression-Aware Loading）

> 根据 Agent 当前 context window 压力（剩余 token 预算）动态调整加载策略：宽裕时 L2，紧张时只 L0。

```rust
// agent-context-db/src/uwu/compression_aware.rs

pub struct CompressionAwareLoader {
    store: Arc<dyn ContextStore>,
    budget_tracker: Arc<ContextBudgetTracker>,
}

#[derive(Debug, Clone)]
pub struct ContextBudgetTracker {
    pub window_total: usize,
    pub window_used: usize,
    pub pressure: PressureLevel,
}

impl ContextBudgetTracker {
    pub fn pressure(&self) -> PressureLevel {
        let ratio = self.window_used as f32 / self.window_total as f32;
        match ratio {
            r if r < 0.3 => PressureLevel::Relaxed,
            r if r < 0.6 => PressureLevel::Moderate,
            r if r < 0.85 => PressureLevel::Tight,
            _ => PressureLevel::Critical,
        }
    }
}

impl CompressionAwareLoader {
    pub async fn load(&self, uri: &ContextUri) -> ContentPayload {
        match self.budget_tracker.pressure() {
            PressureLevel::Relaxed => self.store.read(uri, ContentLevel::L2).await.unwrap(),
            PressureLevel::Moderate => self.store.read(uri, ContentLevel::L1).await.unwrap(),
            PressureLevel::Tight => self.store.read(uri, ContentLevel::L1).await.unwrap(),
            PressureLevel::Critical => self.store.read(uri, ContentLevel::L0).await.unwrap(),
        }
    }

    /// 批量加载：根据预算动态分配层级
    pub async fn load_batch(&self, uris: &[ContextUri]) -> Vec<ContentPayload> {
        let budget = self.budget_tracker.window_total - self.budget_tracker.window_used;
        let mut result = Vec::with_capacity(uris.len());
        let mut remaining = budget;

        for uri in uris {
            if remaining > 2000 {
                result.push(self.store.read(uri, ContentLevel::L1).await.unwrap());
                remaining -= 2000;
            } else if remaining > 100 {
                result.push(self.store.read(uri, ContentLevel::L0).await.unwrap());
                remaining -= 100;
            } else {
                break;  // 预算耗尽，停止加载
            }
        }
        result
    }
}
```

### 1.3 F18 跨 Agent 上下文联邦（Federated Context）

> 多 Agent 共享上下文但不暴露原始数据——通过差分隐私 + 联邦检索，Agent A 可以利用 Agent B 的经验而不读取其原始记忆。

```rust
// agent-context-db/src/federation.rs

pub struct FederatedContext {
    local: Arc<dyn ContextStore>,
    peers: Vec<PeerEndpoint>,
    privacy: PrivacyPolicy,
}

#[derive(Debug, Clone)]
pub struct PeerEndpoint {
    pub agent_id: String,
    pub endpoint: String,
    pub trust_level: TrustLevel,
    pub shared_scopes: Vec<ContextUri>,  // 对方开放的范围
}

#[derive(Debug, Clone)]
pub struct PrivacyPolicy {
    pub dp_epsilon: f32,              // 差分隐私预算
    pub allow_raw: bool,              // 是否允许暴露原始内容
    pub allow_embedding: bool,        // 是否允许暴露向量
    pub allow_metadata_only: bool,    // 只暴露 L0 摘要
}

impl FederatedContext {
    /// 联邦检索：本地 + 对端，结果经隐私处理
    pub async fn federated_retrieve(&self, query: &str, ctx: &RetrieveContext)
        -> Result<FederatedResult, ContextError>
    {
        let local_hits = self.local_retrieve(query, ctx).await?;
        let peer_hits = self.federated_query_peers(query, ctx).await?;
        // 合并 + Rerank + 隐私过滤
        let merged = self.reranker.rerank(query, local_hits.into_iter().chain(peer_hits).collect()).await?;
        Ok(FederatedResult { hits: merged, sources: self.collect_sources() })
    }

    async fn federated_query_peers(&self, query: &str, ctx: &RetrieveContext)
        -> Result<Vec<RetrievalHit>, ContextError>
    {
        let mut all = Vec::new();
        for peer in &self.peers {
            if peer.trust_level == TrustLevel::Blocked { continue; }
            // 只查询对方开放的 scope
            let hits = self.query_peer(peer, query, ctx).await?;
            // 隐私过滤：根据 policy 决定返回 L0/L1/L2
            let filtered = self.apply_privacy(hits, &self.privacy);
            all.extend(filtered);
        }
        Ok(all)
    }
}
```

### 1.4 F19 上下文蒸馏与知识晶体（Knowledge Crystal）

> 高价值上下文经多次引用验证后，自动蒸馏为"知识晶体"——高度压缩、可复用、带可信度评分的结构化知识单元。

```rust
// agent-context-db/src/crystal.rs

/// 知识晶体：蒸馏后的高价值知识单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeCrystal {
    pub id: CrystalId,
    pub uri: ContextUri,               // uwu://.../crystals/{id}
    pub distilled_from: Vec<ContextUri>, // 源上下文
    pub content: CrystalContent,
    pub confidence: f32,               // 蒸馏可信度
    pub verification_count: u32,       // 被验证次数
    pub citation_count: u32,           // 被引用次数
    pub last_verified: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystalContent {
    pub premise: String,       // 前提条件
    pub insight: String,       // 核心洞察（一句话）
    pub evidence: Vec<String>, // 支撑证据
    pub applicability: String, // 适用范围
    pub counter_examples: Vec<String>, // 反例
}

pub struct CrystalDistiller {
    store: Arc<dyn ContextStore>,
    llm: Arc<dyn LlmClient>,
    quality_scorer: Arc<dyn QualityScorer>,
}

impl CrystalDistiller {
    /// 蒸馏触发：某上下文被引用 N 次且质量分 > threshold
    pub async fn try_distill(&self, uri: &ContextUri) -> Result<Option<KnowledgeCrystal>, ContextError> {
        let stats = self.store.access_stats(uri).await?;
        if stats.citation_count < 5 || stats.avg_quality < 0.8 { return Ok(None); }

        // 1. 收集相关上下文
        let related = self.find_related_cluster(uri).await?;
        // 2. LLM 蒸馏
        let content = self.llm.distill_knowledge(&related).await?;
        // 3. 写入 crystals/ 目录
        let crystal = KnowledgeCrystal { /* ... */ };
        self.store.write_crystal(crystal.clone()).await?;
        Ok(Some(crystal))
    }

    /// 晶体验证：新证据出现时重新评估
    pub async fn verify(&self, crystal_id: &CrystalId) -> Result<VerificationResult, ContextError> {
        // 用新证据检验晶体是否仍然成立
    }
}
```

### 1.5 F20 上下文幻觉检测（Hallucination Detector）

> 检测记忆中的矛盾、过时、虚构——通过时序一致性检查 + 跨源交叉验证 + LLM 语义审查。

```rust
// agent-context-db/src/hallucination.rs

pub struct HallucinationDetector {
    store: Arc<dyn ContextStore>,
    llm: Arc<dyn LlmClient>,
    versions: Arc<dyn VersionStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationReport {
    pub uri: ContextUri,
    pub findings: Vec<HallucinationFinding>,
    pub overall_risk: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationFinding {
    pub finding_type: FindingType,
    pub severity: Severity,
    pub description: String,
    pub evidence: Vec<ContextUri>,
    pub suggested_action: SuggestedAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingType {
    Contradiction,      // 与其他记忆矛盾
    Outdated,           // 已被新版本推翻但仍在用
    Unverifiable,       // 无来源支撑
    TemporalInconsistency, // 时间逻辑矛盾
    SemanticDrift,      // 语义漂移（同一概念含义变化）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestedAction {
    Quarantine,    // 隔离，检索时降权
    MarkStale,     // 标记过时
    RequestVerification, // 请求 Agent 重新验证
    AutoCorrect,   // 自动修正（有足够证据时）
    Delete,        // 删除（确认虚构）
}

impl HallucinationDetector {
    /// 对指定范围执行幻觉检测
    pub async fn scan(&self, scope: &ContextUri) -> Result<Vec<HallucinationReport>, ContextError> {
        let entries = self.store.ls(scope).await?;
        let mut reports = Vec::new();
        for entry in entries {
            let report = self.detect_single(&entry.uri).await?;
            if report.overall_risk != RiskLevel::Safe {
                reports.push(report);
            }
        }
        Ok(reports)
    }

    async fn detect_single(&self, uri: &ContextUri) -> Result<HallucinationReport, ContextError> {
        let mut findings = Vec::new();
        // 1. 时序一致性：检查版本历史是否有逻辑矛盾
        findings.extend(self.check_temporal_consistency(uri).await?);
        // 2. 跨源交叉验证：找同类记忆，检查是否矛盾
        findings.extend(self.cross_validate(uri).await?);
        // 3. LLM 语义审查：对高风险条目
        if !findings.is_empty() {
            findings.extend(self.llm_audit(uri).await?);
        }
        Ok(HallucinationReport { uri: uri.clone(), findings, overall_risk: aggregate_risk(&findings) })
    }
}
```

### 1.6 F21 上下文自修复（Self-Healing）

> 发现损坏/不一致时自动修复——利用版本历史 + 因果链 + 备份快照重建一致性状态。

```rust
// agent-context-db/src/healing.rs

pub struct SelfHealer {
    store: Arc<dyn ContextStore>,
    versions: Arc<dyn VersionStore>,
    hallucination_detector: Arc<HallucinationDetector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealingAction {
    pub uri: ContextUri,
    pub issue: HealingIssue,
    pub repair: RepairStrategy,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealingIssue {
    CorruptedContent,      // 内容损坏（反序列化失败）
    BrokenReference,       // 引用指向不存在的 URI
    OrphanedVersion,       // 版本图断裂（parent 不存在）
    InconsistentIndex,     // 向量索引与内容不一致
    StaleL0L1,             // L0/L1 与 L2 不一致
    AclConflict,           // ACL 规则冲突
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RepairStrategy {
    RestoreFromSnapshot { snapshot: SnapshotId },
    RebuildFromParent { parent_commit: CommitId },
    ReindexVector,           // 重建向量索引
    RegenerateL0L1,          // 重新生成 L0/L1
    ReconstructFromProvenance, // 从因果链重建
    QuarantineAndNotify,     // 隔离并通知
}

impl SelfHealer {
    /// 周期性自检
    pub async fn self_check(&self) -> Result<Vec<HealingAction>, ContextError> {
        let mut actions = Vec::new();
        // 1. 完整性校验（内容寻址 hash 比对）
        actions.extend(self.check_integrity().await?);
        // 2. 引用完整性
        actions.extend(self.check_references().await?);
        // 3. 版本图一致性
        actions.extend(self.check_version_graph().await?);
        // 4. 索引一致性
        actions.extend(self.check_index_consistency().await?);
        // 5. 幻觉检测
        let hallucinations = self.hallucination_detector.scan(&root_scope()).await?;
        actions.extend(hallucinations.into_iter().map(|r| self.hallucination_to_healing(r)));
        Ok(actions)
    }

    /// 自动执行高置信度修复，低置信度排队人工审核
    pub async fn execute(&self, actions: Vec<HealingAction>) -> HealingReport {
        let mut auto_fixed = 0;
        let mut manual_required = 0;
        for action in actions {
            if action.confidence > 0.9 {
                self.apply_repair(&action).await;
                auto_fixed += 1;
            } else {
                self.queue_manual(&action).await;
                manual_required += 1;
            }
        }
        HealingReport { auto_fixed, manual_required }
    }
}
```

### 1.7 F22 上下文遗忘曲线（Forgetting Curve）

> 艾宾浩斯遗忘曲线驱动的记忆衰减——未被访问的记忆随时间自然降级，而非硬删除。

```rust
// agent-context-db/src/forgetting.rs

pub struct ForgettingCurve {
    store: Arc<dyn ContextStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgettingModel {
    pub curve_type: CurveType,
    pub stability: f32,        // 记忆稳定性（每次回忆后增加）
    pub retrievability: f32,   // 当前可检索性（0-1，随时间衰减）
    pub last_recalled: chrono::DateTime<chrono::Utc>,
    pub recall_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CurveType {
    /// 艾宾浩斯：R = e^(-t/S)
    Ebbinghaus,
    /// 指数衰减：R = e^(-λt)
    Exponential { lambda: f32 },
    /// 幂律：R = (1 + t/τ)^(-α)
    PowerLaw { tau: f32, alpha: f32 },
    /// 自适应：根据 Agent 风格学习参数
    Adaptive { learned_params: Vec<f32> },
}

impl ForgettingCurve {
    /// 计算某条记忆的当前可检索性
    pub fn retrievability(&self, model: &ForgettingModel, now: chrono::DateTime<Utc>) -> f32 {
        let elapsed = (now - model.last_recalled).num_seconds() as f32 / 86400.0; // 天
        match model.curve_type {
            CurveType::Ebbinghaus => (-elapsed / model.stability).exp(),
            CurveType::Exponential { lambda } => (-lambda * elapsed).exp(),
            CurveType::PowerLaw { tau, alpha } => (1.0 + elapsed / tau).powf(-alpha),
            CurveType::Adaptive { ref p } => self.adaptive_retrievability(p, elapsed),
        }
    }

    /// 回忆时调用：增强记忆稳定性（间隔重复效应）
    pub fn on_recall(&self, model: &mut ForgettingModel) {
        model.recall_count += 1;
        model.last_recalled = Utc::now();
        // 稳定性随回忆次数增长（SM-2 算法变体）
        model.stability *= 1.0 + 0.3 * model.recall_count as f32;
    }

    /// 周期性衰减扫描：retrievability < threshold 的记忆降级
    pub async fn decay_scan(&self, threshold: f32) -> Result<Vec<DecayAction>, ContextError> {
        let all = self.store.scan_all_memories().await?;
        let now = Utc::now();
        let mut actions = Vec::new();
        for (uri, model) in all {
            let r = self.retrievability(&model, now);
            if r < 0.1 {
                actions.push(DecayAction::Archive(uri));
            } else if r < 0.3 {
                actions.push(DecayAction::DegradeToL0(uri));
            } else if r < threshold {
                actions.push(DecayAction::ReduceRerankWeight(uri, r));
            }
        }
        Ok(actions)
    }
}
```

### 1.8 F23 上下文梦境巩固（Dream Consolidation）

> Agent 空闲时离线巩固记忆——类人类睡眠的记忆重组、经验归纳、知识晶体生成。

```rust
// agent-context-db/src/dream.rs

pub struct DreamConsolidator {
    store: Arc<dyn ContextStore>,
    llm: Arc<dyn LlmClient>,
    versions: Arc<dyn VersionStore>,
}

#[derive(Debug, Clone)]
pub struct DreamSession {
    pub session_id: Uuid,
    pub started_at: chrono::DateTime<Utc>,
    pub phase: DreamPhase,
    pub budget: DreamBudget,
}

#[derive(Debug, Clone)]
pub enum DreamPhase {
    /// NREM-浅睡：整理近期轨迹，去重，生成 L0/L1
    LightSleep,
    /// NREM-深睡：跨轨迹归纳经验，生成 KnowledgeCrystal
    DeepSleep,
    /// REM：创意重组，发现隐藏关联，生成假设
    RemSleep,
    /// 清醒：验证 REM 产生的假设，确认或拒绝
    Awake,
}

impl DreamConsolidator {
    /// Agent 空闲时触发（Metacognition.cost_remaining > 0.8 时）
    pub async fn dream(&self, agent_id: &str) -> Result<DreamReport, ContextError> {
        let mut session = DreamSession::new();

        // Phase 1: 浅睡——整理近期轨迹
        let recent = self.fetch_recent_trajectories(agent_id, 24).await?;
        self.consolidate_trajectories(&recent).await?;
        session.advance();

        // Phase 2: 深睡——归纳经验 + 生成晶体
        let clusters = self.cluster_trajectories(&recent).await?;
        for cluster in clusters {
            self.induce_experience(cluster).await?;
            self.try_distill_crystal(cluster).await?;
        }
        session.advance();

        // Phase 3: REM——创意重组
        let hypotheses = self.creative_recombination(&recent).await?;
        session.advance();

        // Phase 4: 清醒——验证假设
        for hyp in &hypotheses {
            self.verify_hypothesis(hyp).await?;
        }

        Ok(DreamReport {
            trajectories_consolidated: recent.len(),
            experiences_induced: clusters.len(),
            crystals_distilled: /* ... */,
            hypotheses_generated: hypotheses.len(),
            hypotheses_verified: /* ... */,
        })
    }

    /// REM 阶段：跨域关联发现
    async fn creative_recombination(&self, trajectories: &[ContextUri])
        -> Result<Vec<Hypothesis>, ContextError>
    {
        // 1. 跨分类检索：在 cases/ 和 patterns/ 间找隐藏关联
        // 2. LLM 生成假设："如果 X 成立，那么 Y 可能也成立"
        // 3. 假设写入 experiences/_hypotheses/
    }
}
```

### 1.9 F24 上下文版本差异推理（Version Diff Reasoning）

> 对比两个版本，不仅给出"变了什么"，更推导出"为什么变了"——基于因果链 + LLM 推理。

```rust
// agent-context-db/src/version/diff_reasoning.rs

pub struct DiffReasoner {
    versions: Arc<dyn VersionStore>,
    llm: Arc<dyn LlmClient>,
    provenance: Arc<dyn ProvenanceStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasonedDiff {
    pub from_commit: CommitId,
    pub to_commit: CommitId,
    pub structural_diff: TreeDiff,       // 结构差异
    pub semantic_changes: Vec<SemanticChange>, // 语义变化
    pub causal_chain: Vec<CausalLink>,   // 因果链
    pub narrative: String,               // LLM 生成的自然语言叙述
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChange {
    pub uri: ContextUri,
    pub change_type: SemanticChangeType,
    pub before_meaning: String,
    pub after_meaning: String,
    pub significance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SemanticChangeType {
    FactUpdated,       // 事实更新
    PreferenceShift,   // 偏好转变
    RelationshipEvolved, // 关系演化
    KnowledgeGained,   // 新增知识
    KnowledgeCorrected, // 知识修正
    ContextOutdated,   // 上下文过时
}

impl DiffReasoner {
    pub async fn reason_diff(&self, from: CommitId, to: CommitId)
        -> Result<ReasonedDiff, VersionError>
    {
        // 1. 结构差异
        let structural = self.versions.diff_commits(&scope, &from, &to).await?;
        // 2. 因果链追溯
        let causal = self.provenance.trace_causal(&from, &to).await?;
        // 3. 语义变化分析
        let semantic = self.analyze_semantic_changes(&structural).await?;
        // 4. LLM 生成叙述
        let narrative = self.llm.narrate_diff(&structural, &semantic, &causal).await?;
        Ok(ReasonedDiff { from_commit: from, to_commit: to, structural_diff: structural, semantic_changes: semantic, causal_chain: causal, narrative })
    }
}
```

### 1.10 F25 上下文安全沙箱（Context Quarantine Sandbox）

> 可疑上下文（来源不可信/幻觉风险高/未验证假设）在 WASM 沙箱中隔离验证后才写入主存储。

```rust
// agent-context-db/src/quarantine.rs

pub struct QuarantineSandbox {
    main_store: Arc<dyn ContextStore>,
    sandbox: Arc<dyn uwu_wasm::Sandbox>,  // 复用 uwu_wasm
    verifier: Arc<dyn ContextVerifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinedEntry {
    pub temp_uri: ContextUri,    // uwu://.../quarantine/{id}
    pub source: EntrySource,
    pub risk_assessment: RiskAssessment,
    pub verification_status: VerificationStatus,
    pub quarantined_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pending,
    InSandbox { started_at: chrono::DateTime<Utc>, checks: Vec<SandboxCheck> },
    Verified { confidence: f32, released_to: ContextUri },
    Rejected { reason: String },
    EscalatedToHuman,
}

impl QuarantineSandbox {
    /// 可疑上下文先入隔离区
    pub async fn quarantine(&self, entry: ContextEntry, source: EntrySource)
        -> Result<QuarantinedEntry, ContextError>
    {
        let risk = self.assess_risk(&entry, &source).await;
        if risk.level == RiskLevel::Safe {
            // 低风险直接写入
            self.main_store.write(entry, "direct").await?;
            return Ok(/* released */);
        }
        // 高风险入隔离区
        let temp_uri = self.write_to_quarantine(entry.clone()).await?;
        // 启动 WASM 沙箱验证
        self.spawn_verification(&temp_uri, &risk).await
    }

    /// 沙箱验证：在隔离环境中测试上下文是否安全/一致
    async fn spawn_verification(&self, uri: &ContextUri, risk: &RiskAssessment) { /* ... */ }

    /// 验证通过后晋升到主存储
    pub async fn release(&self, quarantined: &QuarantinedEntry, target: ContextUri)
        -> Result<CommitId, ContextError>
    {
        // 1. 读取隔离区内容
        // 2. 经 GuardLayer 最终检查
        // 3. 写入主存储
        // 4. 删除隔离区副本
        // 5. 记录因果链
    }
}
```

### 1.11 F26 上下文经济模型（Context Economics）

> 上下文的存储/检索有成本，Agent 有上下文预算——高价值上下文保留，低价值淘汰。

```rust
// agent-context-db/src/economics.rs

pub struct ContextEconomics {
    pricing: PricingModel,
    budget: BudgetAllocator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingModel {
    pub storage_cost_per_mb_per_day: f64,
    pub retrieval_cost_per_token: f64,
    pub llm_cost_per_1k_tokens: f64,
    pub vector_index_cost_per_1k: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextValue {
    pub uri: ContextUri,
    pub storage_cost: f64,       // 存储成本（累计）
    pub retrieval_value: f64,    // 检索价值（被引用带来的收益）
    pub net_value: f64,          // retrieval_value - storage_cost
    pub roi: f64,                // 投入产出比
}

impl ContextEconomics {
    /// 评估某条上下文的经济价值
    pub async fn evaluate(&self, uri: &ContextUri) -> ContextValue {
        let stats = self.store.access_stats(uri).await?;
        let storage = self.compute_storage_cost(uri).await;
        let retrieval = self.compute_retrieval_value(&stats);
        ContextValue {
            uri: uri.clone(),
            storage_cost: storage,
            retrieval_value: retrieval,
            net_value: retrieval - storage,
            roi: if storage > 0.0 { retrieval / storage } else { f64::MAX },
        }
    }

    /// 预算分配：根据 Agent 的上下文预算，决定保留哪些淘汰哪些
    pub async fn allocate_budget(&self, agent_id: &str, budget: f64)
        -> Result<AllocationPlan, ContextError>
    {
        let all = self.store.scan_all(agent_id).await?;
        let mut valued: Vec<_> = futures::future::join_all(
            all.iter().map(|u| self.evaluate(u))
        ).await.into_iter().collect();
        // 按 net_value 降序排列，累计到预算为止
        valued.sort_by(|a, b| b.net_value.partial_cmp(&a.net_value).unwrap());
        let mut keep = Vec::new();
        let mut archive = Vec::new();
        let mut cost = 0.0;
        for v in valued {
            if cost + v.storage_cost <= budget {
                keep.push(v.uri.clone());
                cost += v.storage_cost;
            } else {
                archive.push(v.uri.clone());
            }
        }
        Ok(AllocationPlan { keep, archive, total_cost: cost })
    }
}
```

### 1.12 F27 上下文因果推断（Causal Inference）

> 从变更历史推断因果关系——"X 的变更导致了 Y 的变更"。

```rust
// agent-context-db/src/causal.rs

pub struct CausalInferer {
    changelog: Arc<ChangeLog>,
    llm: Arc<dyn LlmClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalGraph {
    pub nodes: Vec<CausalNode>,
    pub edges: Vec<CausalEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub cause: ContextUri,
    pub effect: ContextUri,
    pub confidence: f32,
    pub mechanism: String,    // 因果机制描述
    pub evidence: Vec<ChangeEvent>,
}

impl CausalInferer {
    /// 从变更历史推断因果关系
    pub async fn infer(&self, scope: &ContextUri, window: TimeRange)
        -> Result<CausalGraph, ContextError>
    {
        let events = self.changelog.replay_range(scope, window).await?;
        // 1. 时序关联：A 变更后 B 频繁变更
        let temporal = self.temporal_correlation(&events).await;
        // 2. 排除混淆变量
        let purified = self.purify_confounders(temporal).await;
        // 3. LLM 推断机制
        let graph = self.llm_infer_mechanism(purified).await?;
        Ok(graph)
    }

    /// 反事实推理："如果 X 没发生，Y 会怎样"
    pub async fn counterfactual(&self, cause_uri: &ContextUri, effect_uri: &ContextUri)
        -> Result<CounterfactualResult, ContextError>
    {
        // 1. 找到 cause 发生的版本
        // 2. 模拟未发生时的状态（asof + 人工干预）
        // 3. 对比 effect 的实际状态 vs 反事实状态
    }
}
```

### 1.13 F28 上下文增量学习（Incremental Retrieval Learning）

> 从检索反馈持续优化检索策略——哪些检索结果被采纳、哪些被忽略，反馈给检索器优化。

```rust
// agent-context-db/src/incremental_learn.rs

pub struct IncrementalLearner {
    retriever: Arc<dyn RetrieverExecutor>,
    feedback_store: Arc<dyn FeedbackStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalFeedback {
    pub query: String,
    pub hits: Vec<ContextUri>,
    pub adopted: Vec<usize>,     // 被采纳的 hit 索引
    pub rejected: Vec<usize>,    // 被明确拒绝的
    pub metacog_score: f32,      // Metacognition 对此次检索的评分
    pub timestamp: chrono::DateTime<Utc>,
}

impl IncrementalLearner {
    /// 记录检索反馈
    pub async fn record_feedback(&self, feedback: RetrievalFeedback) { /* ... */ }

    /// 周期性从反馈学习，调整检索参数
    pub async fn learn(&self) -> Result<LearningResult, ContextError> {
        let feedbacks = self.feedback_store.recent(1000).await?;
        // 1. 分析：哪些目录的检索结果常被采纳/拒绝
        let dir_stats = self.analyze_dir_performance(&feedbacks);
        // 2. 调整：Rerank 权重、目录优先级、intent 路由
        self.adjust_retriever_params(dir_stats).await;
        // 3. 生成个性化检索策略
        Ok(LearningResult { /* ... */ })
    }
}
```

### 1.14 F29 上下文多模态对齐（Multimodal Alignment）

> 文本/图片/音频/视频的语义对齐存储——不同模态的同一概念在向量空间对齐。

```rust
// agent-context-db/src/multimodal.rs

pub struct MultimodalAligner {
    store: Arc<dyn ContextStore>,
    embedding: Arc<dyn MultimodalEmbedder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignedEntry {
    pub concept_uri: ContextUri,    // 概念 URI（模态无关）
    pub modalities: Vec<ModalityEntry>,
    pub alignment_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModalityEntry {
    pub modality: Modality,
    pub uri: ContextUri,
    pub embedding: Vec<f32>,
    pub cross_modal_links: Vec<ContextUri>,
}

impl MultimodalAligner {
    /// 写入多模态内容时自动对齐
    pub async fn write_aligned(&self, concept: &str, entries: Vec<(Modality, Vec<u8>)>)
        -> Result<ContextUri, ContextError>
    {
        // 1. 每个模态生成 embedding
        // 2. 计算跨模态对齐分数
        // 3. 写入各自模态目录 + 概念目录
        // 4. 交叉链接
    }

    /// 跨模态检索：用文本查图片，用图片查音频
    pub async fn cross_modal_search(&self, query_modality: Modality, query: &[u8], target_modality: Modality)
        -> Result<Vec<RetrievalHit>, ContextError>
    {
        // 1. query 生成 embedding
        // 2. 在 target_modality 的向量空间检索（经对齐映射）
    }
}
```

### 1.15 F30 上下文时态推理（Temporal Reasoning）

> 上下文带时间维度，支持"昨天成立的规则今天还成立吗"这类时态推理。

```rust
// agent-context-db/src/temporal.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalContext {
    pub uri: ContextUri,
    pub valid_from: chrono::DateTime<Utc>,
    pub valid_until: Option<chrono::DateTime<Utc>>,  // None = 至今有效
    pub temporal_type: TemporalType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemporalType {
    /// 点事件：某时刻发生
    PointEvent,
    /// 区间事实：某段时间成立
    IntervalFact,
    /// 周期规律：按周期重复
    Periodic { period: chrono::Duration },
    /// 渐进变化：随时间渐变
    Gradual { rate: f32 },
    /// 条件有效：满足条件时有效
    Conditional { condition: String },
}

pub struct TemporalReasoner {
    store: Arc<dyn ContextStore>,
    versions: Arc<dyn VersionStore>,
}

impl TemporalReasoner {
    /// 查询某时间点有效的所有上下文
    pub async fn valid_at(&self, scope: &ContextUri, when: chrono::DateTime<Utc>)
        -> Result<Vec<TemporalContext>, ContextError>;

    /// 时态推理："X 在 T 时刻成立吗"
    pub async fn holds_at(&self, uri: &ContextUri, when: chrono::DateTime<Utc>)
        -> Result<bool, ContextError>;

    /// 检测时态冲突："同一事实在不同时间有矛盾值"
    pub async fn detect_temporal_conflicts(&self, scope: &ContextUri)
        -> Result<Vec<TemporalConflict>, ContextError>;
}
```

---

## 10. 扩展功能

### 3.1 F5：变更事件流 + 因果链（Provenance）

```rust
// agent-context-db/src/version/changelog.rs

/// 变更事件流：所有变更的有序日志（类 Kafka topic）
pub struct ChangeLog {
    subscriber: tokio::sync::broadcast::Sender<ChangeEvent>,
    persistence: Arc<dyn ChangeLogStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    pub event_id: Uuid,
    pub sequence: u64,              // 全局递增序号
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub commit: CommitId,
    pub scope: ContextUri,
    pub change: ChangeSet,
    pub trigger: CommitTrigger,
    pub provenance: Vec<ProvenanceLink>,
}

impl ChangeLog {
    /// 订阅变更流（支持过滤）
    pub fn subscribe(&self, filter: ChangeFilter) -> tokio::sync::broadcast::Receiver<ChangeEvent>;

    /// 回放历史变更
    pub async fn replay(&self, from_seq: u64, filter: &ChangeFilter)
        -> Result<Vec<ChangeEvent>, VersionError>;
}

#[derive(Debug, Clone, Default)]
pub struct ChangeFilter {
    pub scope_prefix: Option<ContextUri>,
    pub categories: Option<Vec<UriCategory>>,
    pub trigger_types: Option<Vec<CommitTrigger>>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
}

/// 因果图：上下文的产生/演化链路
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceGraph {
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceNode {
    pub uri: ContextUri,
    pub commit: CommitId,
    pub kind: ProvenanceNodeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProvenanceNodeKind {
    Source,        // 原始来源（用户输入/外部资源）
    Derived,       // 派生（摘要/提取）
    Aggregated,    // 聚合（经验归纳）
    Merged,        // 合并产物
}

/// 影响分析：某个 commit 影响了哪些后续上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactGraph {
    pub root: CommitId,
    pub affected: Vec<AffectedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedItem {
    pub uri: ContextUri,
    pub commit: CommitId,
    pub relation: ProvenanceRelation,
    pub depth: usize,  // 影响深度
}
```

### 3.2 F7：ContextPack 导出导入

```rust
// agent-context-db/src/pack.rs

/// 上下文打包：子树导出/导入/分享
pub struct ContextPack;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    pub pack_id: Uuid,
    pub name: String,
    pub description: String,
    pub source_tenant: TenantId,
    pub source_uri: ContextUri,
    pub snapshot: SnapshotId,
    pub commit: CommitId,
    pub entry_count: usize,
    pub size_bytes: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub schema_version: String,
    pub integrity: PackIntegrity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackIntegrity {
    pub manifest_hash: ContentHash,
    pub content_hashes: HashMap<ContextUri, ContentHash>,
    pub signature: Option<String>,  // 签名验证
}

impl ContextPack {
    /// 导出子树为 ContextPack
    pub async fn export(&self, scope: &ContextUri, at: VersionRef, opts: &PackOpts)
        -> Result<PackArtifact, VersionError>;

    /// 导入 ContextPack
    pub async fn import(&self, artifact: &PackArtifact, target: &ContextUri, mode: ImportMode)
        -> Result<CommitId, VersionError>;

    /// 增量打包：只含某 commit 后的变更
    pub async fn export_delta(&self, scope: &ContextUri, since: CommitId)
        -> Result<PackArtifact, VersionError>;

    /// 验证 Pack 完整性
    pub async fn verify(&self, artifact: &PackArtifact) -> Result<(), VersionError>;
}

#[derive(Debug, Clone)]
pub enum ImportMode {
    /// 覆盖目标
    Overwrite,
    /// 合并到目标（走 MergeStrategy）
    Merge(MergeStrategy),
    /// 创建新子树
    NewSubtree,
    /// 只读挂载（不复制，引用 Pack）
    ReadOnlyMount,
}
```

### 3.3 F8：路径级 ACL + 版本权限

```rust
// agent-context-db/src/security/acl.rs

/// 路径级访问控制
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEntry {
    pub uri_pattern: ContextUri,    // 支持通配符
    pub principal: Principal,
    pub permissions: Permissions,
    pub priority: i32,              // 多规则优先级
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Principal {
    Tenant(TenantId),
    User(String),
    Agent(String),
    Role(String),  // Orchestrator/Executor/Observer
    Public,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub delete: bool,
    pub branch: bool,       // 创建分支
    pub merge: bool,        // 合并分支
    pub rollback: bool,     // 回滚版本
    pub export: bool,       // 导出 Pack
    pub admin: bool,        // 管理权限
}

/// 版本级权限：控制对历史版本的访问
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionPermission {
    pub commit: CommitId,
    pub visibility: VersionVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionVisibility {
    Public,
    TenantOnly,
    AuthorOnly,
    RoleRestricted(Vec<String>),
    UntilDate(chrono::DateTime<chrono::Utc>),  // 时间锁定
}
```

### 3.4 F9：上下文订阅与增量推送

```rust
// agent-context-db/src/pubsub.rs

/// 上下文变更订阅
pub struct ContextPubSub {
    bus: tokio::sync::broadcast::Sender<ContextEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEvent {
    pub event_id: Uuid,
    pub event_type: ContextEventType,
    pub uri: ContextUri,
    pub commit: CommitId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub payload: ContextEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextEventType {
    Written,
    Deleted,
    Branched,
    Merged,
    Tagged,
    SnapshotTaken,
    L0L1Regenerated,
    MemoryExtracted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextEventPayload {
    Change(ChangeSet),
    NewBranch(Branch),
    MergeResult(MergeResult),
    NewTag(Tag),
    Snapshot(SubtreeSnapshot),
    MemoryUpdate(MemoryDiff),
}

impl ContextPubSub {
    /// 订阅指定路径前缀的变更
    pub fn subscribe(&self, path_prefix: &ContextUri, filter: Option<EventTypeFilter>)
        -> tokio::sync::broadcast::Receiver<ContextEvent>;

    /// 订阅特定 Agent 的所有上下文变更
    pub fn subscribe_agent(&self, agent_id: &str) -> tokio::sync::broadcast::Receiver<ContextEvent>;
}
```

**与 agent-mesh 集成**：跨进程订阅走 NATS，进程内走 tokio broadcast。

### 3.5 F10：上下文 TTL 与生命周期

```rust
// agent-context-db/src/lifecycle.rs

/// 上下文生命周期管理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecyclePolicy {
    pub scope_pattern: ContextUri,
    pub ttl: Option<chrono::Duration>,         // 生存时间
    pub max_access_age: Option<chrono::Duration>, // 最久未访问
    pub degradation: Vec<DegradationRule>,     // 降级规则
    pub on_expire: ExpiryAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationRule {
    pub after: chrono::Duration,
    pub action: DegradeAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DegradeAction {
    /// L2 → 只保留 L0/L1（丢弃详情）
    DropL2,
    /// L1 → 只保留 L0（丢弃概览）
    DropL1,
    /// 移到冷存储
    MoveToCold,
    /// 压缩（squash 版本）
    SquashVersions,
    /// 合并到父级概览
    MergeToParent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpiryAction {
    Delete,
    ArchiveToCold,
    NotifyAndKeep,  // 通知 Agent，由 Agent 决定
    AutoSummarize,  // 压缩为摘要后删除详情
}
```

### 3.6 F11：上下文继承与覆盖

```rust
// agent-context-db/src/inheritance.rs

/// 上下文继承链（类 OOP 继承）
/// 例如：agent 继承自 role 继承自 tenant 全局
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InheritanceChain {
    pub chain: Vec<ContextUri>,  // 从具体到抽象
}

/// 例：
/// uwu://t1/agent/a1/  (具体 Agent)
///   → uwu://t1/role/coder/  (角色)
///     → uwu://t1/tenant/global/  (租户全局)

impl InheritanceChain {
    /// 解析某 URI 的最终值（沿继承链查找，子覆盖父）
    pub async fn resolve(&self, store: &dyn ContextStore, uri: &ContextUri, level: ContentLevel)
        -> Result<ResolvedValue, VersionError>;

    /// 查看某 URI 在继承链各层的值
    pub async fn trace(&self, store: &dyn ContextStore, uri: &ContextUri)
        -> Result<Vec<InheritanceLayer>, VersionError>;
}

#[derive(Debug, Clone)]
pub struct ResolvedValue {
    pub payload: ContentPayload,
    pub source: ContextUri,        // 实际来源层
    pub overridden_by: Vec<ContextUri>,  // 被哪些层覆盖
}

#[derive(Debug, Clone)]
pub struct InheritanceLayer {
    pub uri: ContextUri,
    pub value: Option<ContentPayload>,
    pub is_effective: bool,  // 是否为生效层
}
```

### 3.7 F12：上下文模板

```rust
// agent-context-db/src/template.rs

/// 上下文模板：预定义结构快速实例化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTemplate {
    pub template_id: Uuid,
    pub name: String,
    pub description: String,
    pub tree: TemplateTree,
    pub variables: Vec<TemplateVar>,
    pub hooks: Vec<TemplateHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateTree {
    pub nodes: Vec<TemplateNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateNode {
    pub path: String,  // 相对路径，支持 {var} 插值
    pub content: TemplateContent,
    pub version_strategy: Option<VersionStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemplateContent {
    Static(String),
    Variable(String),       // 引用 template var
    LlmGenerated { prompt: String },
    Empty,                  // 仅创建目录
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVar {
    pub name: String,
    pub var_type: VarType,
    pub default: Option<serde_json::Value>,
    pub required: bool,
}

/// 模板实例化
impl ContextTemplate {
    pub async fn instantiate(&self, store: &dyn ContextStore, target: &ContextUri, vars: &TemplateVars)
        -> Result<CommitId, VersionError>;
}
```

### 3.8 F13：上下文质量评分

```rust
// agent-context-db/src/quality.rs

/// 上下文质量评分：检索结果反馈 + 自动评估
#[async_trait]
pub trait QualityScorer: Send + Sync {
    async fn score(&self, uri: &ContextUri, content: &ContentPayload)
        -> Result<QualityScore, VersionError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityScore {
    pub uri: ContextUri,
    pub overall: f32,
    pub dimensions: QualityDimensions,
    pub scored_at: chrono::DateTime<chrono::Utc>,
    pub scorer_version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityDimensions {
    pub relevance: f32,       // 与查询相关性
    pub freshness: f32,       // 时效性
    pub completeness: f32,    // 完整性
    pub accuracy: f32,        // 准确性（经 Metacog 校准）
    pub usefulness: f32,      // 被引用后的有用度
    pub citation_count: u32,  // 被引用次数
}

/// 质量评分影响检索 Rerank 权重
impl HierarchicalRetrieverImpl {
    async fn quality_adjusted_rerank(&self, hits: Vec<RetrievalHit>)
        -> Result<Vec<RetrievalHit>, ContextError>
    {
        let mut scored = Vec::new();
        for hit in hits {
            let q = self.quality_scorer.score(&hit.uri, &hit.content).await?;
            let adjusted = hit.relevance * 0.7 + q.overall * 0.3;
            scored.push((hit, adjusted));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(scored.into_iter().map(|(h, _)| h).collect())
    }
}
```

### 3.9 F14：上下文去重与相似度聚类

```rust
// agent-context-db/src/dedup.rs

/// 跨 Agent/跨会话记忆去重与聚类
pub struct ContextDeduplicator {
    vector_index: Arc<dyn VectorIndex>,
    llm: Arc<dyn LlmClient>,
}

impl ContextDeduplicator {
    /// 全局去重扫描：找相似度 > threshold 的条目对
    pub async fn find_duplicates(&self, scope: &ContextUri, threshold: f32)
        -> Result<Vec<DuplicateCluster>, VersionError>;

    /// 聚类：相似上下文归组
    pub async fn cluster(&self, scope: &ContextUri, params: &ClusterParams)
        -> Result<Vec<ContextCluster>, VersionError>;

    /// 自动合并可合并的重复项（走 Mergeable 策略）
    pub async fn auto_merge(&self, clusters: Vec<DuplicateCluster>)
        -> Result<MergeReport, VersionError>;
}

#[derive(Debug, Clone)]
pub struct DuplicateCluster {
    pub centroid_uri: ContextUri,
    pub members: Vec<ContextUri>,
    pub avg_similarity: f32,
    pub mergeable: bool,  // 取决于 MemoryClass.mergeable()
}
```

### 3.10 F15：上下文血缘图可视化

```rust
// agent-context-db/src/lineage.rs

/// 血缘图：上下文产生/演化的完整链路可视化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageGraph {
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
    pub layout: GraphLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub id: String,
    pub uri: ContextUri,
    pub commit: CommitId,
    pub label: String,
    pub node_type: LineageNodeType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineageNodeType {
    UserInput,       // 用户输入
    SessionArchive,  // 会话归档
    Memory,          // 提取的记忆
    Trajectory,      // 轨迹
    Experience,      // 经验
    State,           // 状态快照
    Fork,            // 分支
    Merge,           // 合并
    Skill,           // 技能
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEdge {
    pub from: String,
    pub to: String,
    pub relation: ProvenanceRelation,
    pub label: String,
}
```

---

## 11. 风险根治（8 项）

### 2.1 风险 1：L0/L1 生成依赖 LLM，成本高

**根因**：每次内容变更都需 LLM 重新生成摘要/概览，高频写入场景 LLM 调用爆炸。

**v3 根治方案：增量生成 + 模板化 + 投机预生成 + 本地小模型分级**

```rust
// agent-context-db/src/parse/incremental_processor.rs

pub struct IncrementalSemanticProcessor {
    llm_large: Arc<dyn LlmClient>,      // 大模型（复杂摘要）
    llm_small: Arc<dyn LlmClient>,      // 小模型（简单摘要，如 Qwen2.5-0.5B）
    template_engine: TemplateEngine,    // 模板化摘要
    speculator: SpeculativeGenerator,   // 投机预生成
    cache: SemanticCache,               // 生成结果缓存
}

impl IncrementalSemanticProcessor {
    async fn generate_abstract(&self, uri: &ContextUri) -> Result<String, ContextError> {
        let content = self.store.read(uri, ContentLevel::L2).await?;

        // 分级策略：根据内容复杂度选择模型
        let complexity = self.assess_complexity(&content).await;
        match complexity {
            Complexity::Trivial => {
                // 1. 模板化：结构化内容直接模板生成，零 LLM 调用
                Ok(self.template_engine.abstract_from_template(&content))
            }
            Complexity::Simple => {
                // 2. 本地小模型：简单文本用 0.5B 模型，~50ms 无 API 成本
                self.llm_small.complete(&prompt, &LlmOpts::default()).await
            }
            Complexity::Complex => {
                // 3. 大模型：复杂内容才用大模型
                self.llm_large.complete(&prompt, &LlmOpts::default()).await
            }
        }
    }

    /// 增量更新：只重新生成变化部分
    async fn update_overview_incremental(&self, uri: &ContextUri, diff: &ContentDiff)
        -> Result<String, ContextError>
    {
        let old_overview = self.cache.get_overview(uri).await;
        // 只把 diff 部分 + 旧 overview 喂给 LLM，而非全量重新生成
        let prompt = format!("旧概览：{}\n变更：{}\n更新后的概览：", old_overview, diff.summary());
        self.llm_small.complete(&prompt, &LlmOpts::default()).await
    }

    /// 投机预生成：Agent 写入时就预测可能的后续变更，提前生成
    async fn speculative_prefetch(&self, uri: &ContextUri) {
        let patterns = self.speculator.predict_patterns(uri).await;
        for pattern in patterns {
            if let Some(speculative_content) = self.speculator.generate_speculative(uri, &pattern).await {
                self.cache.store_speculative(uri, &pattern, speculative_content).await;
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Complexity { Trivial, Simple, Complex }
```

**量化预期**：LLM 调用降 70-80%（模板40% + 小模型30% + 增量更新20% + 投机命中10%）。

### 2.2 风险 2：DAG 版本图存储成本高

**根因**：每个 commit 存完整 tree_hash 快照，版本数线性增长。

**v3 根治方案：结构化共享 + 增量 tree + 智能压缩 GC**

```rust
// agent-context-db/src/version/compact_store.rs

pub struct CompactVersionStore {
    content_addressed: Arc<dyn ContentAddressedStore>,  // 内容寻址存储
    tree_store: Arc<dyn TreeStore>,
    gc: Arc<VersionGc>,
}

/// 增量 tree：只存与 parent 的 diff，而非完整树
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalTree {
    pub base_commit: CommitId,        // 基线 commit
    pub patches: Vec<TreePatch>,      // 相对基线的补丁
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreePatch {
    Add { path: ContextUri, hash: ContentHash },
    Remove { path: ContextUri },
    Modify { path: ContextUri, old_hash: ContentHash, new_hash: ContentHash },
}

/// 智能压缩 GC
pub struct VersionGc {
    policies: Vec<GcRule>,
}

#[derive(Debug, Clone)]
pub enum GcRule {
    /// 旧版本 squash：超过 N 个版本的前旧版本压缩
    SquashOld { threshold: usize },
    /// 未引用 tree 清理：无 commit 引用的 tree_hash 删除
    PurgeUnreferenced,
    /// 冷版本归档：超期版本移到冷存储
    ArchiveCold { age: chrono::Duration },
    /// 语义去重：相似 commit 合并
    SemanticDedup { similarity: f32 },
}

impl VersionGc {
    async fn run(&self, scope: &ContextUri) -> GcReport {
        // 1. 找出可 squash 的版本段
        let squashable = self.find_squashable(scope).await;
        // 2. 找出未引用的 tree_hash
        let unreferenced = self.find_unreferenced_trees(scope).await;
        // 3. 找出冷版本
        let cold = self.find_cold_commits(scope).await;
        // 4. 语义去重
        let duplicates = self.find_semantic_duplicates(scope).await;
        // 5. 执行
        self.execute(squashable, unreferenced, cold, duplicates).await
    }
}
```

**量化预期**：存储成本降 60-70%（增量tree 50% + unreferenced清理 10% + squash 10%）。

### 2.3 风险 3：时间旅行查询性能

**根因**：ASOF 查询需遍历版本图重建历史状态，延迟高。

**v3 根治方案：物化快照 + 反向 diff + 预计算**

```rust
// agent-context-db/src/version/fast_timetravel.rs

pub struct FastTimeTravel {
    snapshot_scheduler: SnapshotScheduler,  // 定期物化快照
    diff_cache: DiffCache,                  // diff 预计算缓存
    nearest_snapshot: NearestSnapshotIndex, // 最近快照索引
}

impl FastTimeTravel {
    async fn read_at(&self, uri: &ContextUri, when: AsOfTime, level: ContentLevel)
        -> Result<ContentPayload, VersionError>
    {
        // 1. 找最近的物化快照（before when）
        let snapshot = self.nearest_snapshot.find_before(uri, &when).await;
        // 2. 从快照到目标时间点的 diff 链
        let diffs = self.diff_cache.get_range(snapshot.commit, when.commit()).await;
        // 3. 应用 diff 重建（而非从初始版本全量重放）
        let content = self.apply_diffs(snapshot.content(uri), &diffs).await;
        Ok(content)
    }
}

/// 快照调度器：智能选择物化点
pub struct SnapshotScheduler {
    access_pattern: AccessPatternTracker,
}

impl SnapshotScheduler {
    /// 根据访问模式决定何时物化快照
    /// 高频访问的时间段密集快照，低频段稀疏
    async fn schedule_next(&self, scope: &ContextUri) -> SnapshotPlan {
        let hot_periods = self.access_pattern.hot_periods(scope).await;
        // 热点时段每小时一个快照，冷时段每天一个
    }
}
```

**量化预期**：ASOF 查询延迟从秒级降到 < 50ms（快照+少量diff）。

### 2.4 风险 4：LLM 合并仲裁成本

**根因**：合并冲突时每冲突点都调 LLM，大合并成本爆炸。

**v3 根治方案：冲突分类 + 批量仲裁 + 预cedent 学习**

```rust
// agent-context-db/src/version/smart_merge.rs

pub struct SmartMerger {
    llm: Arc<dyn LlmClient>,
    precedent_store: Arc<PrecedentStore>,  // 历史仲裁决策
    classifier: ConflictClassifier,
}

impl SmartMerger {
    async fn merge(&self, conflicts: Vec<MergeConflict>) -> MergeResult {
        // 1. 分类：哪些冲突可以自动解决
        let (auto, llm_required) = self.classifier.classify(&conflicts).await;

        // 2. 自动解决：规则 + precedent
        let mut resolved = Vec::new();
        for c in &auto {
            if let Some(precedent) = self.precedent_store.find_similar(c).await {
                resolved.push(self.apply_precedent(c, precedent));
            } else {
                resolved.push(self.rule_based_resolve(c));
            }
        }

        // 3. 批量 LLM 仲裁：剩余冲突合并为一次 LLM 调用
        if !llm_required.is_empty() {
            let batch_result = self.llm.batch_arbitrate(&llm_required).await?;
            // 4. 仲裁结果存入 precedent（学习）
            for (conflict, decision) in llm_required.iter().zip(batch_result.iter()) {
                self.precedent_store.record(conflict, decision).await;
            }
            resolved.extend(batch_result);
        }

        MergeResult { resolved, unresolved: vec![] }
    }
}

/// Precedent 学习：相似冲突下次自动解决
pub struct PrecedentStore {
    vector_index: Arc<dyn VectorIndex>,
    decisions: Arc<RwLock<HashMap<PrecedentId, Decision>>>,
}
```

**量化预期**：LLM 仲裁调用降 80%（自动+precedent 70% + 批量 10%）。

### 2.5 风险 5：ACL 检查性能

**根因**：每次读写都遍历 ACL 规则，高频访问性能差。

**v3 根治方案：ACL 编译为前缀树 + 权限令牌 + 热路径缓存**

```rust
// agent-context-db/src/security/compiled_acl.rs

pub struct CompiledAcl {
    /// 编译为前缀树，O(path_depth) 查找
    tree: PrefixTree<AclRule>,
    /// 热路径缓存：最近检查的 (uri, principal) → decision
    cache: Arc<RwLock<LruCache<(ContextUri, Principal), Decision>>>,
    /// 权限令牌：会话级令牌，免重复检查
    token_store: TokenStore,
}

impl CompiledAcl {
    /// 编译：规则列表 → 前缀树
    pub fn compile(rules: Vec<AclEntry>) -> Self {
        let mut tree = PrefixTree::new();
        for rule in rules {
            tree.insert(&rule.uri_pattern.0, rule);
        }
        // 预计算默认决策
        Self { tree, cache: Arc::new(RwLock::new(LruCache::new(10000))), token_store: TokenStore::new() }
    }

    /// 检查：前缀树查找 + 缓存 + 令牌
    pub async fn check(&self, uri: &ContextUri, principal: &Principal, perm: Permissions)
        -> Decision
    {
        // 1. 令牌快路径：会话级令牌直接放行
        if let Some(token) = self.token_store.get(principal, uri) {
            if token.covers(perm) { return Decision::Allow; }
        }
        // 2. 缓存
        let key = (uri.clone(), principal.clone());
        if let Some(d) = self.cache.read().await.get(&key) {
            return *d;
        }
        // 3. 前缀树查找（O(path_depth)）
        let rules = self.tree.lookup(&uri.0);
        let decision = self.evaluate(rules, principal, perm);
        // 4. 写缓存 + 可能发令牌
        self.cache.write().await.put(key, decision);
        decision
    }
}

/// 权限令牌：一次验证，会话内有效
pub struct PermissionToken {
    pub principal: Principal,
    pub scope: ContextUri,     // 令牌覆盖范围
    pub permissions: Permissions,
    pub expires_at: chrono::DateTime<Utc>,
}
```

**量化预期**：ACL 检查延迟从 ms 级降到 μs 级（前缀树 + 缓存命中率 > 95%）。

### 2.6 风险 6：删除 Sidecar 后崩溃隔离丢失

**根因**：SemanticQueue 内嵌主进程，LLM 调用 panic 可能拖垮主进程。

**v3 根治方案：进程内隔离 + 监督树 + 降级策略**

```rust
// agent-context-db/src/compressor/supervised_queue.rs

pub struct SupervisedSemanticQueue {
    /// 独立 tokio runtime：SemanticQueue worker 在独立线程池
    worker_runtime: tokio::runtime::Runtime,
    /// 监督树
    supervisor: Supervisor,
    /// 降级策略
    fallback: FallbackStrategy,
}

impl SupervisedSemanticQueue {
    pub fn new() -> Self {
        // 独立 runtime：worker panic 不影响主 runtime
        let worker_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("semantic-worker")
            .enable_all()
            .build()
            .unwrap();

        Self {
            worker_runtime,
            supervisor: Supervisor::new(),
            fallback: FallbackStrategy::new(),
        }
    }

    /// 提交任务：主 runtime → worker runtime
    pub async fn enqueue(&self, task: SemanticTask) -> Result<TaskId, ContextError> {
        // 跨 runtime 发送
    }
}

/// 监督树：worker 崩溃自动重启 + 降级
pub struct Supervisor {
    restart_policy: RestartPolicy,
    health_check: HealthChecker,
}

#[derive(Debug, Clone)]
pub enum RestartPolicy {
    /// OneForOne：崩溃的 worker 重启
    OneForOne { max_restarts: usize, window: chrono::Duration },
    /// SimpleOneForOne：所有 worker 重启
    AllForOne,
    /// 降级运行：崩溃后用简化模式
    Degrade { degraded_fn: DegradedFn },
}

/// 降级策略：LLM 不可用时用规则兜底
pub struct FallbackStrategy {
    rules: Vec<FallbackRule>,
}

#[derive(Debug, Clone)]
pub enum FallbackRule {
    /// LLM 不可用 → 用模板生成 L0（粗糙但可用）
    LlmUnavailableUseTemplate,
    /// 向量索引不可用 → 全文检索降级
    VectorUnavailableUseFullText,
    /// PG 不可用 → 写本地 WAL，恢复后重放
    PgUnavailableUseWal,
}
```

**量化预期**：worker 崩溃 0 影响主进程（独立 runtime + 监督树），降级保证可用性 > 99.9%。

### 2.7 风险 7：五维持久化并发问题

**根因**：多维度同时读写，MVCC 粒度不足导致冲突。

**v3 根治方案：维度级锁 + 乐观并发 + 冲突检测**

```rust
// agent-context-db/src/uwu/concurrency.rs

pub struct DimensionalLockManager {
    /// 按维度+scope 分锁，细粒度并发
    locks: DashMap<(StateScope, ScopeKey), Arc<RwLock<()>>>,
}

impl DimensionalLockManager {
    /// 乐观并发：先读版本号，写时检查
    pub async fn optimistic_write<F, T>(&self, uri: &ContextUri, f: F) -> Result<T, ConcurrencyError>
    where
        F: AsyncFnOnce(&AgentState) -> T,
    {
        loop {
            let version = self.store.read_version(uri).await;
            let snapshot = self.store.read(uri).await;
            let result = f(&snapshot).await;
            // CAS：版本号未变则提交
            match self.store.cas_write(uri, &snapshot, version).await {
                Ok(_) => return Ok(result),
                Err(CasError::Conflict) => continue,  // 重试
            }
        }
    }

    /// 维度级读锁：允许多维同时读
    pub async fn read_lock(&self, scope: StateScope, key: &ScopeKey)
        -> ReadGuard
    {
        let lock = self.locks.entry((scope, key.clone()))
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone();
        lock.read().await
    }
}
```

### 2.8 风险 8：LLM 去重决策误判

**根因**：LLM 对相似但不同的记忆可能误删/误合并。

**v3 根治方案：三重校验 + 软删除 + 可回滚 + 置信度门控**

```rust
// agent-context-db/src/parse/safe_dedup.rs

pub struct SafeDeduplicator {
    llm: Arc<dyn LlmClient>,
    vector_index: Arc<dyn VectorIndex>,
    /// 三重校验：向量相似 + 语义校验 + LLM 决策
    validators: Vec<Box<dyn DedupValidator>>,
    /// 软删除：标记而非真删
    soft_delete: SoftDeleteStore,
}

#[async_trait]
pub trait DedupValidator: Send + Sync {
    async fn validate(&self, candidate: &MemoryCandidate, existing: &[MemoryCandidate])
        -> ValidatorVerdict;
}

#[derive(Debug, Clone)]
pub enum ValidatorVerdict {
    ConfirmMerge,
    ConfirmKeep,
    Uncertain,
    Reject,
}

impl SafeDeduplicator {
    async fn deduplicate(&self, candidates: Vec<MemoryCandidate>)
        -> Result<Vec<DedupDecision>, ContextError>
    {
        let mut decisions = Vec::new();
        for candidate in &candidates {
            // 1. 向量相似度初筛
            let similar = self.vector_index.find_similar(&candidate.embedding, 0.85).await;
            if similar.is_empty() {
                decisions.push(DedupDecision::create(candidate));
                continue;
            }
            // 2. 三重校验
            let mut votes = Vec::new();
            for v in &self.validators {
                votes.push(v.validate(candidate, &similar).await);
            }
            // 3. 置信度门控：全票通过才执行
            let confidence = self.aggregate_confidence(&votes);
            if confidence > 0.9 && votes.iter().all(|v| matches!(v, ValidatorVerdict::ConfirmMerge)) {
                // 高置信度：软删除（标记 deleted=true，保留 30 天）
                decisions.push(DedupDecision::soft_merge(candidate, &similar[0]));
            } else if confidence > 0.6 {
                // 中置信度：标记待审
                decisions.push(DedupDecision::pending_review(candidate));
            } else {
                // 低置信度：保留
                decisions.push(DedupDecision::keep(candidate));
            }
        }
        Ok(decisions)
    }
}
```

**量化预期**：误判率从 ~15% 降到 < 1%（三重校验 + 置信度门控）。

---

## 12. 性能优化（10 项）

### 3.1 P1 检索查询编译器（RetrieverCompiler）

> 破坏性重构：HierarchicalRetriever 拆分为编译器+执行器，查询编译为可缓存的执行计划。

```rust
// agent-context-db/src/retrieve/compiler.rs

/// 检索查询编译器：查询 → 执行计划
pub struct RetrieverCompiler {
    plan_cache: Arc<RwLock<LruCache<QueryHash, Arc<ExecutionPlan>>>>,
    optimizer: QueryOptimizer,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct QueryHash(pub u64);

/// 执行计划：编译后的检索步骤序列
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub hash: QueryHash,
    pub steps: Vec<PlanStep>,
    pub estimated_cost: Cost,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone)]
pub enum PlanStep {
    VectorSearch { query: String, scope: ContextUri, top_k: usize },
    DirListing { dir: ContextUri },
    IntraDirSearch { dir: ContextUri, query: String },
    RecursiveDescent { from: ContextUri, depth: usize },
    Rerank { model: String, input_size: usize },
    Load { level: ContentLevel, budget: usize },
    Filter { predicate: Predicate },
    Merge { sources: Vec<PlanStep> },
}

impl RetrieverCompiler {
    pub async fn compile(&self, query: &str, ctx: &RetrieveContext)
        -> Result<Arc<ExecutionPlan>, ContextError>
    {
        let hash = hash_query(query, ctx);
        // 1. 缓存命中
        if let Some(plan) = self.plan_cache.read().await.get(&hash) {
            return Ok(plan.clone());
        }
        // 2. 编译
        let raw_plan = self.parse_to_plan(query, ctx).await?;
        // 3. 优化
        let optimized = self.optimizer.optimize(raw_plan).await;
        // 4. 缓存
        let plan = Arc::new(optimized);
        self.plan_cache.write().await.put(hash, plan.clone());
        Ok(plan)
    }
}

/// 执行器：执行编译好的计划
pub struct RetrieverExecutor {
    store: Arc<dyn ContextStore>,
    vector_index: Arc<dyn VectorIndex>,
    reranker: Arc<dyn Reranker>,
}

impl RetrieverExecutor {
    pub async fn execute(&self, plan: &ExecutionPlan) -> Result<RetrievalResult, ContextError> {
        let mut results = Vec::new();
        for step in &plan.steps {
            match step {
                PlanStep::VectorSearch { query, scope, top_k } => {
                    results.extend(self.vector_index.search(query, scope, *top_k).await?);
                }
                PlanStep::Rerank { .. } => {
                    results = self.reranker.rerank(&query_text, results).await?;
                }
                // ...
            }
        }
        Ok(RetrievalResult { /* ... */ })
    }
}
```

**收益**：相同查询计划复用，省编译时间；优化器选择最优执行路径。

### 3.2 P2 向量索引分层（Hot/Warm/Cold）

```rust
// agent-context-db/src/storage/layered_vector.rs

pub struct LayeredVectorIndex {
    hot: Arc<dyn VectorIndex>,    // 内存索引（近期高频访问）
    warm: Arc<dyn VectorIndex>,   // Qdrant（常规）
    cold: Arc<dyn VectorIndex>,   // 压缩存储（历史冷数据）
    promoter: AccessPromoter,
}

impl LayeredVectorIndex {
    async fn search(&self, query: &str, scope: &ContextUri, top_k: usize) -> Vec<SearchHit> {
        // 1. 先查 hot
        let mut hits = self.hot.search(query, scope, top_k).await;
        if hits.len() < top_k {
            // 2. 补查 warm
            let warm_hits = self.warm.search(query, scope, top_k - hits.len()).await;
            hits.extend(warm_hits);
            // 3. 不足再查 cold
            if hits.len() < top_k {
                let cold_hits = self.cold.search(query, scope, top_k - hits.len()).await;
                hits.extend(cold_hits);
            }
        }
        // 4. 访问促升：cold → warm → hot
        self.promoter.promote_accessed(&hits).await;
        hits
    }
}
```

### 3.3 P3 嵌入向量量化压缩（PQ/SQ）

```rust
// agent-context-db/src/storage/quantized_vector.rs

/// 乘积量化压缩：1024 维 float32 (4KB) → 64 字节，压缩 64x
pub struct QuantizedVectorIndex {
    pq: ProductQuantizer,
    full_index: Arc<dyn VectorIndex>,  // 原始索引（精确重排用）
}

impl QuantizedVectorIndex {
    async fn search(&self, query: &str, top_k: usize) -> Vec<SearchHit> {
        // 1. 用压缩向量快速召回 top_k * 10
        let candidates = self.pq.search(query, top_k * 10).await;
        // 2. 用原始向量精确重排
        let reranked = self.full_index.rerank(query, &candidates, top_k).await;
        reranked
    }
}
```

**收益**：内存占用降 64x，检索延迟降 3-5x（压缩向量扫描快）。

### 3.4 P4 L0/L1 流水线并行生成

```rust
// agent-context-db/src/parse/pipeline.rs

pub struct ParallelSemanticPipeline {
    stages: Vec<PipelineStage>,
}

#[derive(Debug, Clone)]
pub enum PipelineStage {
    Parse,           // 解析内容
    Embed,           // 生成向量
    Abstract,        // 生成 L0
    Overview,        // 生成 L1
    Aggregate,       // 向上聚合
}

impl ParallelSemanticPipeline {
    /// 流水线并行：不同条目的不同阶段并行执行
    pub async fn process_batch(&self, entries: Vec<ContextUri>) {
        // Stage 1: 所有条目并行解析
        let parsed = futures::future::join_all(
            entries.iter().map(|e| self.parse(e))
        ).await;
        // Stage 2: 解析完成的立即开始 embed（不等所有解析完）
        let embedded = self.pipeline(parsed, |e| self.embed(e)).await;
        // Stage 3: embed 完成的立即开始 abstract
        // ... 形成流水线
    }
}
```

### 3.5 P5 写前日志(WAL) + 批量提交

```rust
// agent-context-db/src/storage/wal.rs

pub struct WalStore {
    wal: Arc<MappedFile>,      // 内存映射 WAL 文件
    batch_buffer: Vec<WriteOp>,
    batch_size: usize,
    commit_handle: tokio::task::JoinHandle<()>,
}

impl WalStore {
    /// 写入：先写 WAL（顺序写，极快），再异步批量提交到 PG
    pub async fn write(&self, op: WriteOp) -> Result<(), ContextError> {
        // 1. 写 WAL（fsync 保证持久性）
        self.wal.append(&op).await?;
        // 2. 加入批量缓冲
        self.batch_buffer.push(op);
        if self.batch_buffer.len() >= self.batch_size {
            self.flush_batch().await?;
        }
        Ok(())
    }

    /// 批量提交：积攒 N 条后一次 PG 事务
    async fn flush_batch(&self) -> Result<(), ContextError> {
        let batch = std::mem::take(&mut self.batch_buffer);
        self.pg.batch_insert(&batch).await?;
        // WAL 截断
        self.wal.truncate().await?;
    }
}
```

### 3.6 P6 检索结果物化视图

```rust
// agent-context-db/src/retrieve/materialized.rs

pub struct MaterializedView {
    store: Arc<dyn ContextStore>,
    views: Arc<RwLock<HashMap<ViewKey, MaterializedResult>>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ViewKey {
    pub query_hash: QueryHash,
    pub scope: ContextUri,
}

impl MaterializedView {
    /// 高频查询的结果物化，TTL 过期
    pub async fn get_or_compute(&self, key: &ViewKey, compute: impl AsyncFnOnce() -> RetrievalResult)
        -> RetrievalResult
    {
        if let Some(cached) = self.views.read().await.get(key) {
            if !cached.is_expired() {
                return cached.result.clone();
            }
        }
        let result = compute().await;
        self.views.write().await.insert(key.clone(), MaterializedResult {
            result: result.clone(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        });
        result
    }

    /// 上下文变更时失效相关视图
    pub async fn invalidate(&self, changed_scope: &ContextUri) {
        let mut views = self.views.write().await;
        views.retain(|key, _| !key.scope.starts_with(&changed_scope.0));
    }
}
```

### 3.7 P7 零拷贝读取路径

```rust
// agent-context-db/src/storage/zerocopy.rs

pub struct ZeroCopyReader {
    mmap_cache: Arc<RwLock<HashMap<ContentHash, Arc<MappedFile>>>>,
}

impl ZeroCopyReader {
    /// 零拷贝读取：mmap 直接映射文件，避免 kernel→userspace 拷贝
    pub async fn read(&self, hash: &ContentHash) -> Result<ZeroCopyBuf, ContextError> {
        if let Some(mmap) = self.mmap_cache.read().await.get(hash) {
            return Ok(ZeroCopyBuf::from_mmap(mmap.clone()));
        }
        let path = self.hash_to_path(hash);
        let file = tokio::fs::File::open(&path).await?;
        let mmap = unsafe { MappedFile::map(&file)? };
        let mmap = Arc::new(mmap);
        self.mmap_cache.write().await.insert(hash.clone(), mmap.clone());
        Ok(ZeroCopyBuf::from_mmap(mmap))
    }
}

/// 零拷贝缓冲区：直接引用 mmap 内存
pub struct ZeroCopyBuf {
    mmap: Arc<MappedFile>,
    offset: usize,
    len: usize,
}

impl ZeroCopyBuf {
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap[self.offset..self.offset + self.len]
    }
}
```

### 3.8 P8 LlmClient 流式 + 批量 + 投机（破坏性升级）

```rust
// agent-context-db/src/uwu/llm.rs (v3 升级)

#[async_trait]
pub trait LlmClient: Send + Sync {
    // v1/v2 方法保留...

    /// v3 新增：流式生成（L0/L1 边生成边写入，降低首字节延迟）
    async fn stream_complete(
        &self, prompt: &str, opts: &LlmOpts
    ) -> Result<Box<dyn LlmStream>, LlmError>;

    /// v3 新增：批量调用（多个摘要请求合并为一次 LLM 调用）
    async fn batch_complete(
        &self, prompts: &[String], opts: &LlmOpts
    ) -> Result<Vec<String>, LlmError>;

    /// v3 新增：投机执行（同时发起大模型+小模型，小模型先出就用小模型结果）
    async fn speculative_complete(
        &self, prompt: &str, opts: &LlmOpts
    ) -> Result<String, LlmError>;
}

#[async_trait]
pub trait LlmStream: Send + Sync {
    async fn next_chunk(&mut self) -> Option<Result<String, LlmError>>;
}

/// 投机执行器：大模型和小模型并行，小模型快则用小模型，大模型验证
pub struct SpeculativeExecutor {
    large: Arc<dyn LlmClient>,
    small: Arc<dyn LlmClient>,
}

impl SpeculativeExecutor {
    async fn speculative_complete(&self, prompt: &str) -> String {
        let large_fut = self.large.complete(prompt, &opts);
        let small_fut = self.small.complete(prompt, &opts);
        tokio::select! {
            Ok(small_result) = small_fut => {
                // 小模型先返回，先用，大模型结果作为验证
                // 如果大模型后续返回差异大，触发修正
                small_result
            }
            Ok(large_result) = large_fut => {
                // 大模型先返回（小模型超时），直接用
                large_result
            }
        }
    }
}
```

### 3.9 P9 上下文分区与并行检索

```rust
// agent-context-db/src/retrieve/partition.rs

pub struct PartitionedRetriever {
    partitions: Vec<ContextPartition>,
}

pub struct ContextPartition {
    pub scope: ContextUri,
    pub local_index: Arc<dyn VectorIndex>,
}

impl PartitionedRetriever {
    /// 并行检索所有分区，结果合并
    pub async fn retrieve(&self, query: &str, top_k: usize) -> Vec<SearchHit> {
        let per_partition = (top_k / self.partitions.len()).max(1);
        let results = futures::future::join_all(
            self.partitions.iter().map(|p| p.local_index.search(query, &p.scope, per_partition))
        ).await;
        // 合并 + 全局 top_k
        let mut all: Vec<_> = results.into_iter().flatten().collect();
        all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        all.truncate(top_k);
        all
    }
}
```

### 3.10 P10 内容寻址存储的压缩与去重

```rust
// agent-context-db/src/storage/cas.rs

pub struct ContentAddressedStore {
    blob_store: BlobStore,
    dedup_index: Arc<RwLock<HashMap<ContentHash, BlobRef>>>,
}

impl ContentAddressedStore {
    /// 写入：内容 hash 去重
    pub async fn write(&self, content: &[u8]) -> Result<ContentHash, ContextError> {
        let hash = blake3::hash(content);
        let hash = ContentHash(hash.to_hex().to_string());
        // 去重：相同内容只存一份
        if self.dedup_index.read().await.contains_key(&hash) {
            return Ok(hash);  // 已存在
        }
        // 压缩存储
        let compressed = zstd::encode_all(content, 3)?;
        let blob_ref = self.blob_store.write(&compressed).await?;
        self.dedup_index.write().await.insert(hash.clone(), blob_ref);
        Ok(hash)
    }

    /// 读取
    pub async fn read(&self, hash: &ContentHash) -> Result<Vec<u8>, ContextError> {
        let blob_ref = self.dedup_index.read().await.get(hash).cloned()
            .ok_or(ContextError::NotFound)?;
        let compressed = self.blob_store.read(&blob_ref).await?;
        let decompressed = zstd::decode_all(&compressed[..])?;
        Ok(decompressed)
    }
}
```

**收益**：相同内容零重复存储 + zstd 压缩降 50-70% 空间。

---


---


---

### 架构总览图（含创新功能与性能优化）

```
┌──────────────────────────────────────────────────────────────────────┐
│ Agent 决策层 (uwu_agent_engine 五维 + FlowGraph + Metacognition)       │
├──────────────────────────────────────────────────────────────────────┤
│ 上下文认知层 (v3 新增)                                                 │
│  PredictivePrefetcher │ CompressionAwareLoader │ FederatedContext     │
│  CrystalDistiller     │ HallucinationDetector │ SelfHealer           │
│  ForgettingCurve      │ DreamConsolidator     │ DiffReasoner         │
│  QuarantineSandbox    │ ContextEconomics      │ CausalInferer        │
│  IncrementalLearner   │ MultimodalAligner     │ TemporalReasoner     │
├──────────────────────────────────────────────────────────────────────┤
│ 检索层 (v3 重构: 编译器+执行器)                                         │
│  RetrieverCompiler ─→ ExecutionPlan ─→ RetrieverExecutor              │
│  QueryOptimizer │ MaterializedView │ PartitionedRetriever             │
├──────────────────────────────────────────────────────────────────────┤
│ 版本管理层 (v2 + v3 性能优化)                                           │
│  VersionStore (DAG) │ FastTimeTravel │ CompactVersionStore │ SmartMerger│
├──────────────────────────────────────────────────────────────────────┤
│ 语义处理层 (v3 增量+分级)                                               │
│  IncrementalSemanticProcessor │ ParallelSemanticPipeline              │
│  SafeDeduplicator │ QuarantineSandbox                                   │
│  SupervisedSemanticQueue (独立 runtime + 监督树)                        │
├──────────────────────────────────────────────────────────────────────┤
│ 存储层 (v3 分层+量化+零拷贝)                                            │
│  ContentAddressedStore (去重+zstd) │ WalStore (WAL+批量)               │
│  LayeredVectorIndex (hot/warm/cold) │ QuantizedVectorIndex (PQ)        │
│  ZeroCopyReader (mmap) │ CompiledAcl (前缀树)                          │
└──────────────────────────────────────────────────────────────────────┘
```

---



---

### 量化收益汇总

### 6.1 性能指标

| 指标 | v1/v2 | v3 目标 | 优化手段 |
|---|---|---|---|
| 检索延迟 P99 | ~200ms | < 50ms | 编译器+物化视图+分区+Prefetcher |
| LLM 调用次数 | 基线 | 降 70-80% | 增量生成+模板+小模型+投机+batch |
| 向量索引内存 | 基线 | 降 64x | PQ 量化 |
| 版本存储成本 | 基线 | 降 60-70% | 增量tree+去重+GC |
| ASOF 查询延迟 | 秒级 | < 50ms | 物化快照+反向diff |
| ACL 检查延迟 | ms 级 | μs 级 | 前缀树+令牌+缓存 |
| 写入吞吐 | 基线 | 提 3-5x | WAL+批量提交 |
| 内容存储空间 | 基线 | 降 50-70% | CAS去重+zstd |

### 6.2 可靠性指标

| 指标 | v1/v2 | v3 目标 |
|---|---|---|
| Worker 崩溃影响主进程 | 可能 | 0（独立runtime+监督树） |
| LLM 去重误判率 | ~15% | < 1%（三重校验+门控） |
| 自修复覆盖率 | 0 | > 90%（周期自检+自动修复） |
| 幻觉检测覆盖率 | 0 | > 80%（时序+跨源+LLM审查） |
| 可用性（LLM 故障时） | 降级 | 降级运行（模板+规则兜底） |

---


## 13. JEPA 预测器落地方案

### 13.1 三条路线对比

| 路线 | 原理 | 训练成本 | 推理成本 | 精度 | 落地难度 |
|---|---|---|---|---|---|
| 经典JEPA | 自监督训练 | 高 | 低 | 高 | 高 |
| LLM-as-Predictor | LLM直接预测 | 零 | 中 | 中 | 低 |
| 轻量统计模型 | 访问频率统计 | 零 | 极低 | 低 | 极低 |

### 13.2 JepaPredictor trait（多实现）

四种实现：LlmJepaPredictor / OnnxJepaPredictor / StatisticalPrefetcher / HybridJepaPredictor。

```rust
#[async_trait]
pub trait JepaPredictor: Send + Sync {
    async fn predict_next_contexts(&self, state: &AgentState, history: &[ContextAccessRecord]) -> Vec<Prediction>;
    async fn calibrate(&self, predictions: &[Prediction], actual: &[ContextUri]) -> CalibrationResult;
}
```

### 13.3 渐进落地

阶段1（立即）：StatisticalPrefetcher + LlmJepaPredictor
阶段2（3个月后）：引入 OnnxJepaPredictor（AgentWorld 35B 蒸馏到 1.5B）
阶段3（6个月后）：HybridJepaPredictor 微调

### 13.4 开源模型选择

| 模型 | 用途 | 部署方式 | 硬件需求 |
|---|---|---|---|
| AgentWorld 35B | 教师模型 | HTTP API | 云端GPU |
| R1-Distill-Qwen-1.5B | 学生模型 | GGUF | CPU 8GB |
| Qwen2.5-0.5B | L0/L1生成 | ONNX | CPU 2GB |

### 13.5 家用机部署

| 档位 | CPU | 内存 | GPU | 性能 |
|---|---|---|---|---|
| 最低 | 4核 | 8GB | 无 | ~5 tok/s |
| 主流 | 8核 | 16GB | 无 | ~15 tok/s |
| 高端 | 16核 | 32GB | RTX 4060 | ~60 tok/s |

### 13.6 蒸馏方案

3个域模型 + 1.5B甜点，一次性蒸馏成本 ~$200。


---

## 14. LLM 成本模型与部署方案

### 14.1 6 个 LLM 调用点

| 调用点 | 频率 | Token | 优化 |
|---|---|---|---|
| L0摘要 | 每次写入 | ~100 | 模板/小模型 |
| L1概览 | 每次聚合 | ~2k | 增量 |
| 记忆提取 | 每次commit | ~500 | 批量 |
| 去重决策 | 每次提取 | ~300 | 三重校验 |
| 意图分析 | 每次检索 | ~200 | 规则 |
| 合并仲裁 | 每次冲突 | ~500 | precedent |

### 14.2 API 定价对比

| API | 输入($/1M) | 输出($/1M) |
|---|---|---|
| GPT-4o-mini | $0.15 | $0.60 |
| DeepSeek-V3 | $0.14 | $0.28 |
| 本地0.5B | $0 | $0 |

### 14.3 优化后成本（降85%）

中活跃：$36 → $5.4/月；高活跃：$180 → $27/月

### 14.4 零GPU方案：< $1/天


---

## 15. 实施路线图

### 15.1 基础架构路线图

### 9.1 分阶段计划

#### 阶段 1: 基础设施（2-3 周）
- 新建 `agent-context-db` crate 骨架
- 实现 `ContextStore` trait（PG 后端 `AgfsStore`）
- 实现 `VectorIndex` trait（Qdrant 后端）
- 实现 `FsOps`（ls/find/grep/read/tree）
- 实现 `LlmClient` trait + MCP/Direct 双实现
- **破坏性影响**：无（新 crate 独立）
- **关键依赖**：PG schema 迁移、Qdrant collection 初始化

#### 阶段 2: URI 与三层模型（2 周）
- 定义 `ContextUri`、`ContextEntry`、`ContentLevel`
- 实现 L0/L1/L2 读写
- 实现 `SemanticProcessor`（LLM 生成 abstract/overview，走 LlmClient）
- **破坏性影响**：无

#### 阶段 3: 检索机制（2-3 周）
- 实现 `HierarchicalRetriever`
- 实现 `IntentAnalyzer`（LLM + 规则双实现）
- 实现 `Reranker`（cross-encoder）
- 实现 `RetrievalTrace` + tracing 集成
- **破坏性影响**：`agent-memory` 的 retrieve/retrieve_typed 委托新 API（标记旧 API deprecated）
- **关键依赖**：阶段 1、2

#### 阶段 4: 会话压缩与异步管线（3 周）★ 破坏性最大
- 实现 `SessionCompressor`（两阶段 commit）
- 实现 `SemanticQueue`（tokio mpsc + worker pool）
- 实现 `MemoryExtractor`（8 种分类 + LLM 去重）
- 实现 `TrajectoryExtractor`（轨迹/经验两层）
- **删除** `agent-sidecar-consolidator`
- **重构** `agent-session`：`checkpoints` → `commit_phase1/2`
- **破坏性影响**：Sidecar 进程消失，需迁移现有部署
- **关键依赖**：阶段 1-3

#### 阶段 5: 五维融合（3-4 周）
- 实现 `StateBridge`（含 fork 沙盒）
- 实现 `PersonaBridge`（关系图谱查询）
- 实现 `MetacogBridge`（校准数据检索）
- 实现 `CharacterConstraint`（写入约束）
- **重构** `agent-state` / `agent-persona`：持久化下沉
- **破坏性影响**：五维 crate 持久化层重写（内存结构保留）
- **关键依赖**：阶段 1-4

#### 阶段 6: Wiki 集成（uwu_wiki）（2-3 周）
- **删除** `agent-wiki` crate
- 引入 `uwu_wiki` 依赖（feature: `llm-workflow`, `wiki-collab`）
- 在 `agent-context-db` 中实现 `WikiStorage` trait（对接 PG+Qdrant 双层存储）
- 初始化 `WikiSpace`，注入 `ContextDbWikiStorage`
- 实现 `uwu://agent-x/wiki/{doc_id}/{block_id}` URI 映射
- 接入 LLM Wiki 工作流：Ingest / Query（含反写策略）/ Lint（定时审计）
- 迁移原 wiki 调用方到 `WikiSpace` API
- 发布 `wiki.ingest.*` / `wiki.lint.*` 事件到 `agent-mesh`
- **破坏性影响**：所有 wiki 调用方需迁移；Block 树替代扁平 WikiPage
- **关键依赖**：阶段 5；uwu_wiki v2.1（存储层外部注入）

#### 阶段 7: 升级点（2-3 周）
- Metacog 校准检索（U2）
- Guard 写入闸门集成（U3）
- Reaction 自动学习（U10）
- 多租户（U6）
- WASM 沙箱衍生计算（U12）
- **关键依赖**：阶段 5

#### 阶段 8: 集成测试与性能基线（1-2 周）
- 端到端：session.commit → context-db → retrieval → metacog 校准
- 性能基线对比（对比旧 UnifiedMemory 的 token 消耗/检索延迟）
- **无旧数据迁移**（uwu 仍在设计阶段）

### 9.2 关键依赖链

```
阶段1 ─→ 阶段2 ─→ 阶段3 ─┐
                         ├─→ 阶段4 ──┐
                         │           ├─→ 阶段5 ─→ 阶段6 ─→ 阶段7 ─→ 阶段8
                         └───────────┘
```

阶段 4 是关键路径瓶颈（破坏性最大），阶段 5 依赖 1-4 全部完成。

### 9.3 每阶段破坏性影响范围

| 阶段 | 删除 crate | 重构 crate | 部署影响 |
|---|---|---|---|
| 1-3 | 无 | agent-memory (deprecated 标记) | 无 |
| 4 | agent-sidecar-consolidator | agent-session, agent-learning | ★ Sidecar 进程下线 |
| 5 | 无 | agent-state, agent-persona, agent-metacognition, agent-character | 持久化层迁移 |
| 6 | agent-wiki | 所有 wiki 调用方 | wiki API 迁移到 WikiSpace；Block 树替代 WikiPage |
| 7-8 | 无 | agent-reaction (新增 learner) | 无 |

---

### 15.2 版本管理路线图

升级在 v1 路线图基础上新增阶段：

| 阶段 | 内容 | 周期 | 依赖 |
|---|---|---|---|
| **阶段-1** | 版本模型重构（Commit DAG + 分支 + 标签） | 3 周 | v1 阶段 1-2 |
| **阶段-2** | VersionStore trait + PG schema 升级（内容寻址存储） | 2 周 | 阶段-1 |
| **阶段-3** | 合并策略（三路 + CRDT + LLM 仲裁） | 2 周 | 阶段-2 |
| **阶段-4** | 子树快照 + 时间旅行（ASOF 查询） | 2 周 | 阶段-2 |
| **阶段-5** | 变更事件流 + 因果链 + 血缘图 | 2 周 | 阶段-2 |
| **阶段-6** | ContextPack 导出导入 | 1.5 周 | 阶段-2 |
| **阶段-7** | 路径级 ACL + 版本权限 | 1.5 周 | 阶段-2 |
| **阶段-8** | 上下文订阅 + 增量推送 | 1 周 | 阶段-5 |
| **阶段-9** | TTL + 生命周期 + 降级 | 1.5 周 | 阶段-2 |
| **阶段-10** | 继承链 + 模板 | 2 周 | v1 阶段 5 |
| **阶段-11** | 质量评分 + 去重聚类 | 2 周 | v1 阶段 3 |
| **阶段-12** | 五维融合升级（ToT 多分支 + 版本感知校准） | 2 周 | 阶段-3, 阶段-4, v1 阶段 5 |
| **阶段-13** | 集成测试 + 性能基线 | 1.5 周 | 全部 |
| **合计** | | **24.5 周** | |

### 关键依赖链

```
v2-1 → v2-2 → v2-3 ─┐
                     ├─→ v2-4 ─┐
                     ├─→ v2-5 ─┤
                     ├─→ v2-6 ─┤
                     ├─→ v2-7 ─┼─→ v2-12 → v2-13
                     ├─→ v2-9 ─┘
                     └─→ v2-8 (依赖 v2-5)
v1 阶段 5 → v2-10
v1 阶段 3 → v2-11
```

### 风险与缓解

| 风险 | 缓解 |
|---|---|
| DAG 版本图存储成本高 | 内容寻址 + 去重（相同 tree_hash 复用）+ 定期 GC squash |
| 时间旅行查询性能 | tree_hash 缓存 + 增量 diff 预计算 + 冷热分离 |
| LLM 合并仲裁成本 | 仅冲突部分走 LLM，可合并部分自动解决；LLM 结果缓存 |
| 多分支并发写冲突 | 分支隔离 + merge 时解决；Branchable 策略限制 max_branches |
| ACL 检查性能 | 路径模式编译 + 缓存 + default_deny 快速拒绝 |
| 生命周期误删关键上下文 | on_expire=NotifyAndKeep 兜底 + 重要条目打 immutable tag |

---

### 15.3 创新与优化路线图

| 阶段 | 内容 | 周期 | 依赖 |
|---|---|---|---|
| **阶段-1** | 性能重构：RetrieverCompiler + 执行计划 + 物化视图 | 3 周 | v2 完成 |
| **阶段-2** | 存储优化：LayeredVector + PQ量化 + CAS去重 + WAL + 零拷贝 | 3 周 | 阶段-1 |
| **阶段-3** | LlmClient 升级：stream + batch + speculative | 2 周 | 阶段-1 |
| **阶段-4** | 风险根治 1-4：增量生成 + CompactVersion + FastTimeTravel + SmartMerger | 4 周 | 阶段-2 |
| **阶段-5** | 风险根治 5-8：CompiledAcl + SupervisedQueue + 并发控制 + SafeDedup | 3 周 | 阶段-2 |
| **阶段-6** | 创新功能 A：Prefetcher + CompressionAware + Federation | 3 周 | 阶段-1 |
| **阶段-7** | 创新功能 B：Crystal + Hallucination + SelfHealer | 3 周 | 阶段-4 |
| **阶段-8** | 创新功能 C：Forgetting + Dream + DiffReasoning | 3 周 | 阶段-4 |
| **阶段-9** | 创新功能 D：Quarantine + Economics + Causal + Incremental + Multimodal + Temporal | 4 周 | 阶段-5 |
| **阶段-10** | 集成测试 + 性能基线 + 五维融合验证 | 2 周 | 全部 |
| **合计** | | **30 周** | |

### 总周期

```
v1(17-22周) + v2(24.5周) + v3(30周) ≈ 71-76 周
可并行重叠约 35%，实际 ≈ 46-50 周
```

---

### 15.4 统一路线图

| 阶段 | 内容 | 周期 | 依赖 |
|---|---|---|---|
| 1 | 基础设施 | 2-3周 | 无 |
| 2 | URI与三层模型 | 2周 | 1 |
| 3 | 检索机制 | 2-3周 | 1-2 |
| 4 | 会话压缩 | 3周 | 1-3 |
| 5 | 五维融合 | 3-4周 | 1-4 |
| 6 | Wiki合并 | 2周 | 5 |
| 7 | 版本模型 | 3周 | 1-2 |
| 8 | VersionStore | 2周 | 7 |
| 9 | 合并策略 | 2周 | 8 |
| 10 | 快照+时间旅行 | 2周 | 8 |
| 11 | 变更事件流 | 2周 | 8 |
| 12 | ContextPack+ACL | 2-3周 | 8 |
| 13 | 继承+模板 | 2-3周 | 5 |
| 14 | RetrieverCompiler | 3周 | 3 |
| 15 | 存储优化 | 3周 | 14 |
| 16 | LlmClient升级 | 2周 | 14 |
| 17 | 风险根治1-4 | 4周 | 15 |
| 18 | 风险根治5-8 | 3周 | 15 |
| 19 | 创新A-C | 4周 | 17 |
| 20 | 创新D | 4周 | 18 |
| 21 | 集成测试 | 2周 | 全部 |

总周期约 50-60 周，实际 ≈ 40-48 周。


---

## 16. 配置示例

```toml
# uwu_agent_engine.toml

[context_db]
storage_backend = "dual"  # pg + qdrant
tenant_isolation = true

[context_db.storage.agfs]
url = "postgres://localhost/uwu_context_db"
pool_size = 20

[context_db.storage.vector]
url = "http://localhost:6334"
collection = "uwu_context"
dimension = 1024

[context_db.llm]
default_impl = "mcp"  # 或 "direct"
[context_db.llm.mcp]
server_id = "uwu-llm-server"
[context_db.llm.direct]
provider = "openai"
api_base = "https://api.openai.com/v1"
model = "gpt-4o-mini"

[context_db.semantic_queue]
worker_parallelism = 4
queue_capacity = 256
backpressure_block = true  # 满载时 commit_phase1 同步阻塞

[context_db.retrieval]
default_prefer_level = "L1"
rerank_model = "cross-encoder"
intent_analyzer = "llm"  # 或 "rule"
trace_enabled = true

[context_db.session]
compression_threshold_msgs = 50
max_archive_depth = 100
```

---

### 版本管理配置

```toml
# uwu_agent_engine.toml (v2 增量)

[context_db.version]
default_strategy = "branchable"  # 全局默认
auto_snapshot_on_step = true      # 每步推理自动快照
max_branches_per_scope = 16
fork_auto_cleanup = true
gc_interval_secs = 3600

[context_db.version.policies]
# 按 UriCategory 覆盖
sessions = "immutable"
state = "branchable"
wiki = "branchable"
metacog = "linear_mvcc"
events = "immutable"
cases = "immutable"

[context_db.version.merge]
default_strategy = "three_way"
conflict_resolver = "llm_arbitrate"
llm_impl = "mcp"  # 冲突仲裁用哪个 LLM

[context_db.version.retention]
default_max_versions = 100
default_max_age_days = 30
gc_action = "squash"

[context_db.pubsub]
enabled = true
cross_process_via_nats = true
buffer_size = 1024

[context_db.lifecycle]
enabled = true
scan_interval_secs = 600
[context_db.lifecycle.default]
ttl_days = 90
degradation = [
  { after_days = 30, action = "drop_l2" },
  { after_days = 60, action = "move_to_cold" },
]
on_expire = "auto_summarize"

[context_db.acl]
enabled = true
default_deny = true
cache_ttl_secs = 300

[context_db.pack]
export_compression = "zstd"
max_pack_size_mb = 512
signature_required = false  # 内部使用可关
```

---


---

## 17. 关键文件路径清单

```
agent-context-db/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── client/            # L1 客户端层
│   │   ├── mod.rs
│   │   └── client.rs      # ContextDbClient (第一版仅嵌入式)
│   ├── service/           # L2 服务层
│   │   ├── mod.rs
│   │   ├── service.rs     # ContextDbService
│   │   ├── uri_resolver.rs
│   │   └── permission.rs
│   ├── retrieve/          # L3 检索层
│   │   ├── mod.rs
│   │   ├── retriever.rs   # HierarchicalRetriever
│   │   ├── intent.rs      # IntentAnalyzer
│   │   ├── reranker.rs
│   │   └── trace.rs       # RetrievalTrace
│   ├── session/           # L4 会话层
│   │   ├── mod.rs
│   │   ├── compressor.rs  # SessionCompressor
│   │   └── archiver.rs
│   ├── parse/             # L5 解析层
│   │   ├── mod.rs
│   │   ├── processor.rs   # SemanticProcessor
│   │   ├── extractor.rs   # MemoryExtractor
│   │   └── trajectory.rs  # TrajectoryExtractor
│   ├── compressor/        # L6 压缩层
│   │   ├── mod.rs
│   │   ├── queue.rs       # SemanticQueue
│   │   └── scheduler.rs
│   ├── storage/           # L7 存储层
│   │   ├── mod.rs
│   │   ├── store.rs       # ContextStore trait + AgfsStore
│   │   ├── vector.rs      # VectorIndex + Qdrant 实现
│   │   ├── fs_ops.rs      # ls/find/grep/read/tree
│   │   └── mvcc.rs
│   └── uwu/               # L8 升级层（五维融合 + LLM 抽象）
│       ├── mod.rs
│       ├── llm.rs         # LlmClient trait + McpLlmClient + DirectLlmClient
│       ├── state_bridge.rs
│       ├── persona_bridge.rs
│       ├── metacog_bridge.rs
│       ├── character_constraint.rs
│       ├── reaction_learner.rs
│       ├── crdt_merger.rs
│       ├── guard_integrator.rs
│       └── fork_feeder.rs
├── migrations/
│   ├── 001_agfs_schema.sql       # PG 表结构
│   ├── 002_mvcc_tables.sql
│   └── 003_qdrant_collections.json
└── tests/
    ├── integration_retrieve.rs
    ├── integration_commit.rs
    └── integration_five_dim.rs
```

---


---

## 18. 总结

本方案以 agent-context-db 新 crate 为核心，实现：

- 完全复刻 OpenViking：FS范式、L0/L1/L2、8种记忆分类、LLM去重、轨迹/经验两层、两阶段commit
- 类Git DAG版本管理：Commit+Branch+Tag+Merge、快照、时间旅行、因果链
- 15项创新功能：预测预加载/压缩感知/联邦/知识晶体/幻觉检测/自修复/遗忘曲线/梦境巩固/版本差异推理/安全沙箱/经济模型/因果推断/增量学习/多模态对齐/时态推理
- 8项风险根治：LLM成本/存储成本/时间旅行/合并仲裁/ACL/崩溃隔离/并发/去重
- 10项性能优化：查询编译器/向量分层/PQ量化/流水线/WAL/物化视图/零拷贝/流式LLM/分区/CAS
- 五维深度融合：State/Persona/Metacog/Character/Reaction 皆为FS一等目录
- JEPA预测器：多实现，渐进落地
- LLM成本模型：降85%，零GPU可运行

总周期约 40-48 周。
