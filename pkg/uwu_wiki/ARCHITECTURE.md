# uwu_wiki — 智能知识库系统架构

> **破坏性重构版本**（v2）
> 核心变更：高性能存储模型重设计、wiki-graph 全面对齐 wiki-llm 能力集、
> 索引层统一、Arc\<RwLock\> 并发模型贯穿所有 Store、删除冗余抽象。
> **v2.1 新增**：LLM Wiki 工作流（Ingest / Query / Lint 三操作管线）、存储层外部注入（去除自带存储实现）。

---

## 目录

1. [设计原则](#1-设计原则)
2. [架构全景](#2-架构全景)
3. [核心模块 wiki-core](#3-核心模块-wiki-core)
4. [文档模型 Block 体系](#4-文档模型-block-体系)
5. [智能表格 wiki-table](#5-智能表格-wiki-table)
6. [智能图 wiki-graph](#6-智能图-wiki-graph)
7. [LLM 横切层 wiki-llm](#7-llm-横切层-wiki-llm)
8. [LLM Wiki 工作流](#8-llm-wiki-工作流)
9. [协作层 wiki-collab](#9-协作层-wiki-collab)
10. [存储层外部注入](#10-存储层外部注入)
11. [事件总线集成](#11-事件总线集成)
12. [Crate 拆分与 Feature 矩阵](#12-crate-拆分与-feature-矩阵)
13. [依赖关系图](#13-依赖关系图)
14. [配置参考](#14-配置参考)

---

## 1. 设计原则

| 原则 | 说明 |
|---|---|
| **Block 第一** | 文档、表格、图均由 Block 树组成；Block 是 LLM 检索、CRDT 合并、事件发布的最小单元 |
| **索引先行** | 所有热路径（按 tag / category / status / title / graph_id 过滤）走倒排索引，零全表扫描 |
| **并发安全** | 所有 Store 内部使用 `Arc<RwLock<Inner>>`；`.clone()` 即可跨任务共享，无外部锁 |
| **统一 LLM 接口** | wiki-llm 的 `LlmAction` 体系同时覆盖文档、表格、图；wiki-graph 不另起炉灶 |
| **增量 Embedding** | 只重算变更节点及其一跳邻居，其余节点 embedding 复用缓存 |
| **可插拔存储** | VectorStore 后端通过 trait 抽象，由外部调用方注入；uwu_wiki 不持有存储实例 |
| **Op 日志驱动** | 所有写操作产生 Op；Op 既是 CRDT 合并输入，也是事件总线消息体，也是审计日志 |
| **LLM Wiki 工作流** | LLM 是 wiki 的全职编辑：Ingest（原料→知识）/ Query（问答→反写）/ Lint（定期审计），知识随时间复利增长 |

---

## 1.5 内部模块化解耦原则（破坏性修正）

> 现设计存储注入(§10)做得干净,但内部仍有三处耦合缺陷会阻碍独立发布与单测。本节立约束并给出破坏性修正。

**缺陷 1：LLM 调用硬绑 agent-core。** §7 文档 RAG 管道写死"→ agent-core LLM 调用",wiki-llm 因此强依赖引擎,无法作为通用库独立发布,单测必须起真实 LLM。
- **修正**：wiki-llm 依赖注入的 `LlmClient` trait（复用 `agent-context-db` 已定义的同名 trait,同一抽象不重复造),由调用方在构造期注入。wiki-llm 内部零 `use agent_core`。单测注入 mock LlmClient。

**缺陷 2：wiki-core 内嵌 storage 实现。** §10.2 把 `MemoryWikiStorage` 放在 `wiki-core/src/storage/`,让"零外部依赖的纯 Block 引擎"混入了存储职责,破坏 wiki-core 的纯粹性。
- **修正**：`wiki-core` 只留 `WikiStorage`/`DocStore`/`OpLog`/`VectorStore` 的 **trait 定义**(端口),不含任何实现。`MemoryWikiStorage` 移到独立 `wiki-testkit` crate（dev-dependency),生产与测试都不污染核心。

**缺陷 3：wiki-llm 反向知道 table/graph 领域类型。** `LlmDocAction` / `LlmGraphAction` 把三个领域(文档/表格/图)的动作枚举塞进 wiki-llm,使横切层反向依赖具体领域,违反单向依赖。
- **修正**：wiki-llm 只定义领域无关的 `LlmCapability` 端口(embed / search / complete / qa / summarize,输入输出均为通用 `TextUnit { id, text, path }`)。各领域(table/graph)在**自己的 crate**里把领域实体适配成 `TextUnit` 并调用 wiki-llm 端口。wiki-llm 不再 `use` 任何领域类型。

**四条硬约束（编译期由 crate 边界强制）**：
1. **端口/适配器**：每个 crate 对外只暴露 trait(端口);后端/引擎是适配器,构造期注入。
2. **单向依赖**：`wiki-core`(纯) ← `wiki-llm`/`wiki-table`/`wiki-graph`/`wiki-collab` ← 调用方。横切层不得反向依赖领域层。
3. **依赖倒置**：`LlmClient`/`WikiStorage` 全 trait 注入,任何 crate 不 `use` 引擎或存储的具体 struct。
4. **核心纯粹性**：`wiki-core` 除 serde/uuid/chrono 零依赖,不含存储实现、不含 LLM 调用。

---

## 2. 架构全景

```
┌────────────────────────────────────────────────────────────────────────┐
│                              uwu_wiki                                  │
│                                                                        │
│  ┌─────────────┐   ┌──────────────┐   ┌─────────────────────────────┐  │
│  │  wiki-core  │   │  wiki-table  │   │        wiki-graph           │  │
│  │  Block 引擎  │   │   智能表格    │   │  流程图 / 思维导图 + LLM    │  │
│  └──────┬──────┘   └──────┬───────┘   └─────────────┬───────────────┘  │
│         │                 │                         │                  │
│  ┌──────┴─────────────────┴─────────────────────────┴──────────────┐   │
│  │                         wiki-llm                                │   │
│  │   LlmAction（文档/表格/图统一入口）  Embedding · RAG · 搜索 · 补全  │   │
│  │                                                                  │   │
│  │  ┌──────────────────────────────────────────────────────────┐   │   │
│  │  │              LLM Wiki 工作流（wiki-workflow）              │   │   │
│  │  │   Ingest（原料→知识）· Query（问答→反写）· Lint（审计）     │   │   │
│  │  └──────────────────────────────────────────────────────────┘   │   │
│  └────────────────────────────┬────────────────────────────────────┘   │
│                               │                                        │
│  ┌────────────────────────────┴────────────────────────────────────┐   │
│  │                        wiki-collab                              │   │
│  │              CRDT 协作 · 权限控制 · Op 广播 · 离线队列            │   │
│  └────────────────────────────┬────────────────────────────────────┘   │
└───────────────────────────────┼────────────────────────────────────────┘
                                │
          ┌─────────────────────┼─────────────────────┐
          ▼                     ▼                     ▼
   VectorStore trait      uwu_event_mesh          uwu_wasm
  (外部注入，不持有实例)   (跨进程事件总线)       (WASM 插件沙箱，可选)
```

---

## 3. 核心模块 wiki-core

最小可运行核心，零外部依赖（除 serde / uuid / chrono）。

```
wiki-core/src/
├── block.rs       Block + BlockId + BlockContent + BlockMeta
├── doc.rs         Document + DocId + SpaceId + 版本号
├── op.rs          Op 枚举（Insert / Update / Delete / Move / DocMeta）
├── registry.rs    BlockTypeRegistry（注册自定义 Block 类型）
├── render.rs      Block → Markdown / JSON / HTML 渲染 trait
└── lib.rs
```

**内置 Block 类型：**

| BlockType | 说明 |
|---|---|
| `paragraph` | 文本段落（inline 富文本标记） |
| `heading` | 标题 h1-h6 |
| `bulleted_list` / `numbered_list` | 列表 |
| `toggle` | 折叠块 |
| `quote` / `callout` | 引用 / 高亮提示框 |
| `code` | 代码块（含语言元数据） |
| `divider` / `image` / `embed` | 分隔线 / 图片 / 外部嵌入 |
| `table_ref` | 指向 wiki-table 实例的引用块 |
| `graph_ref` | 指向 wiki-graph 实例的引用块 |
| `database_view` | 数据库视图（table / kanban / gallery / timeline） |

新类型通过 `BlockTypeRegistry::register` 注册，核心不硬编码具体类型。

---

## 4. 文档模型 Block 体系

```rust
pub struct Block {
    pub id: BlockId,                   // UUID v7（时间有序，便于排序）
    pub ty: BlockType,
    pub content: BlockContent,         // serde_json::Value 封装，类型特定
    pub children: Vec<BlockId>,        // 有序子块列表
    pub parent: Option<BlockId>,
    pub version: u64,                  // 乐观并发版本号
    pub embedding: Option<Vec<f32>>,   // 懒生成，LLM Worker 异步填充
    pub meta: BlockMeta,               // created_at / updated_at / created_by
}

pub struct Document {
    pub id: DocId,
    pub title: String,
    pub root: BlockId,
    pub version: u64,
    pub space_id: SpaceId,
    pub tags: Vec<String>,
    pub icon: Option<String>,
    pub cover: Option<String>,
}

pub enum Op {
    InsertBlock { parent: BlockId, after: Option<BlockId>, block: Block },
    UpdateBlock { id: BlockId, patch: serde_json::Value },
    DeleteBlock { id: BlockId },
    MoveBlock   { id: BlockId, new_parent: BlockId, after: Option<BlockId> },
    UpdateDocMeta { doc_id: DocId, patch: serde_json::Value },
}
```

### 4.1 双向链接与反向引用（补充 #2）

Block 正文可含 `[[wiki-link]]` 引用其它文档/Block。核心维护一张**引用图**,支撑 backlinks（"哪些页面引用了本页"）——这是 LLM 做知识关联的关键信号,也是现代 wiki 的核心能力。

```rust
/// 页面内联引用（解析自 [[target]] 语法或显式 mention）
pub struct WikiLink {
    pub from: BlockId,          // 引用发起 Block
    pub to: LinkTarget,         // 引用目标
    pub anchor_text: String,    // 显示文本
}

pub enum LinkTarget {
    Doc(DocId),
    Block(DocId, BlockId),
    Broken(String),            // 悬空引用（目标不存在），Lint 可修复
}

/// 引用图：正向 + 反向双索引，wiki-core 维护
pub trait LinkGraph: Send + Sync {
    /// 本 Block/Doc 引用了谁（正向）
    fn outbound(&self, from: BlockId) -> Vec<WikiLink>;
    /// 谁引用了本 Doc/Block（反向，即 backlinks）
    fn backlinks(&self, target: &LinkTarget) -> Vec<WikiLink>;
    /// 全库悬空引用（供 Lint 审计）
    fn broken_links(&self) -> Vec<WikiLink>;
}
```

- 链接在 `UpdateBlock`/`InsertBlock` 时由核心解析并增量更新引用图,不额外走 LLM。
- 删除目标 Block 时,指向它的链接自动标记为 `Broken`,由 §8.4 Lint 的 `missing_page` 流程处理。
- 引用图持久化走 `WikiStorage`,新增 `link_store()` 端口（见 §10.1）。

---

## 5. 智能表格 wiki-table

```
wiki-table/src/
├── schema.rs      Table / Column / ColumnType
├── row.rs         Row / Cell / CellValue
├── formula.rs     公式引擎（复用 uwu_visual_script SlotProgram）
├── view.rs        TableView（过滤 / 排序 / 分组 / 隐藏列）
├── llm_col.rs     LLM 列（AI 自动填充）
└── lib.rs
```

### 列类型

| ColumnType | 说明 |
|---|---|
| `Text` / `Number` / `Checkbox` / `Url` / `Date` | 基础类型 |
| `Select` / `MultiSelect` | 枚举选项 |
| `Relation` | 关联另一张表（外键语义） |
| `Rollup` | 对 Relation 列聚合（sum / count / avg） |
| `Formula` | 使用 uwu_visual_script 图节点表达，编译为 SlotProgram |
| `LlmFill` | LLM 智能填充（见下） |
| `CreatedAt` / `UpdatedAt` / `CreatedBy` | 系统字段 |

### LLM 列（LlmFill）

```rust
pub struct LlmFillConfig {
    pub prompt_template: String,    // 支持 {column_name} 占位符
    pub model: String,
    pub trigger: FillTrigger,       // OnCreate | OnUpdate(Vec<ColumnId>) | Manual
    pub output_type: ColumnType,
}
```

填充流程：`行写入 → wiki.llm.fill_request 事件 → LLM Worker → 写回 Cell`

---

## 6. 智能图 wiki-graph

`wiki-graph` 完整对齐 `wiki-llm` 的能力集：**以图节点为检索单元**，支持全套 LLM 操作（生成 / 搜索 / 问答 / 摘要 / 改写 / 标注 / 补全）。

```
wiki-graph/src/
├── model.rs            GraphDoc / GraphNode / GraphEdge / GraphStyle
├── store.rs            GraphStore（Arc<RwLock<Inner>> + 倒排索引）
├── layout.rs           布局算法 trait（Dagre / Force / Radial / TreeMap）
├── mindmap.rs          思维导图专用模型（RootNode / Branch / 折叠状态）
├── flowchart.rs        流程图节点类型（Start / End / Decision / Process / SubProcess）
├── export.rs           导出 SVG / PNG / Mermaid / PlantUML
├── op.rs               图 Op 日志（NodeInsert / EdgeInsert / NodeUpdate / Delete）
└── llm/
    ├── mod.rs          LlmGraphAction 枚举（统一入口，与 LlmDocAction 对称）
    ├── embed.rs        节点/图级 embedding 生成与增量更新
    ├── search.rs       语义搜索（向量检索 + 标签混合，top-k + graph_id 过滤）
    ├── generate.rs     TextToGraph / Expand / Convert / ExtractFromDoc
    ├── qa.rs           图 RAG 问答（节点为检索单元，邻居扩展 context）
    ├── summarize.rs    整图 / 子图摘要
    ├── rewrite.rs      节点内容改写（风格 / 语言 / 精简）
    ├── tag.rs          节点自动打标签 / 聚类分析
    └── complete.rs     节点标签行内补全
```

### 图节点类型

**流程图（`flowchart`）：**

| NodeShape | 说明 |
|---|---|
| `start` / `end` | 圆角矩形，流程起止 |
| `process` | 矩形，普通步骤 |
| `decision` | 菱形，条件判断 |
| `io` | 平行四边形，输入输出 |
| `sub_process` | 双边矩形，子流程引用 |
| `annotation` | 注释气泡 |

**思维导图（`mindmap`）：**

树形结构，节点含 `label / color / icon / collapsed / embedding`，支持无限嵌套。每个节点独立参与 LLM 索引，可被语义搜索精确命中。

### LlmGraphAction — 完整 LLM 能力集

```rust
pub enum LlmGraphAction {
    // —— 生成类 ————————————————————————————————————————
    /// 文本/文档 Block → 自动生成流程图或思维导图
    TextToGraph { text: String, graph_type: GraphType },
    /// 选中节点 → 续写新分支（思维导图扩展 / 流程图后续步骤）
    Expand { node_id: NodeId, direction: ExpandDir, depth: u8 },
    /// 图类型互转（流程图 ↔ 思维导图）
    Convert { target_type: GraphType },
    /// 从外部文档/URL 抽取结构 → 生成图
    ExtractFromDoc { source: GraphSource },

    // —— 问答 / 检索类 ——————————————————————————————————
    /// 图 RAG 问答：节点为检索单元，回答含引用 NodeId
    Ask { question: String, graph_id: Option<GraphId> },
    /// 语义搜索：返回最相关节点列表（含相似度评分）
    Search { query: String, top_k: usize, graph_id: Option<GraphId> },

    // —— 理解 / 摘要类 ——————————————————————————————————
    /// 整图或选中子图 → 自然语言摘要
    Summarize { scope: GraphScope },
    /// 节点内容改写（风格 / 语言 / 精简）
    Rewrite { node_id: NodeId, style: RewriteStyle },
    /// 节点自动打标签（用于分组 / 着色 / 过滤）
    AutoTag { scope: GraphScope },
    /// 语义聚类：将相近节点归组，建议分层结构
    Cluster { scope: GraphScope },

    // —— 补全类 ————————————————————————————————————————
    /// 节点标签行内补全（用户输入时实时调用）
    Complete { node_id: NodeId, partial: String },
}
```

### GraphStore — 高性能内存存储

与 `MemoryWikiStore` 同构，`Arc<RwLock<Inner>>` + 倒排索引：

```rust
struct GraphInner {
    graphs: HashMap<GraphId, GraphDoc>,
    nodes:  HashMap<NodeId, GraphNode>,   // 全局节点索引（跨图检索）
    edges:  HashMap<EdgeId, GraphEdge>,
    // 倒排索引
    graph_nodes:  HashMap<GraphId, HashSet<NodeId>>,   // O(1) 按图查节点
    tag_nodes:    HashMap<String, HashSet<NodeId>>,    // O(1) 按标签查节点
    type_nodes:   HashMap<NodeShape, HashSet<NodeId>>, // O(1) 按类型查节点
}
```

### Embedding 策略

检索单元：**节点**（非整图）

```
节点标签 + 注释文本
  + 父路径文本（思维导图）
  / 上下游节点标签（流程图，1 跳）
  → 拼装上下文文本
  → wiki-llm::embed 生成 embedding
  → uwu_database VectorStore upsert
    collection: "wiki_graph_nodes"
    metadata:   { graph_id, node_id, graph_type, tags }
```

**增量维护**（只算变更节点及其一跳邻居）：

```
NodeInsert / NodeUpdate Op
  → wiki.graph.node.updated 事件
  → LLM Worker：重算该节点 + 邻居节点 embedding
  → upsert 到 VectorStore（其余节点 embedding 不动）
```

### 图 RAG 管道

```
用户问题
  → wiki-graph::llm::search
      向量检索 top-k 节点（可按 graph_id 过滤）
  → 邻居扩展（search_context_hops，默认 1）
      拉取命中节点的直接邻居，提升结构连贯性
  → 构建 context prompt
      图类型 + 节点路径 + 边关系 + 节点文本
  → wiki-llm → agent-core LLM 调用
  → 返回答案 + 引用 NodeId 列表（前端可高亮对应节点）
```

---

## 7. LLM 横切层 wiki-llm

统一覆盖文档 Block、表格行、图节点三类实体,不重复实现。

> **解耦要点(见 §1.5 缺陷 3)**：wiki-llm **只认领域无关的 `TextUnit`**,不 `use` 文档/表格/图的具体类型。各领域 crate 负责把自己的实体适配成 `TextUnit` 再调用本层端口。LLM 后端由注入的 `LlmClient` 提供,wiki-llm 不依赖 agent-core。

```rust
/// 领域无关的文本单元——三类实体(Block/表格行/图节点)统一适配成它
pub struct TextUnit {
    pub id: String,        // 领域实体 ID 的字符串化(BlockId/RowId/NodeId)
    pub text: String,      // 待处理文本
    pub path: Vec<String>, // 溯源路径(doc→block / table→row / graph→node)
}

/// LLM 能力端口——领域无关,输入输出均为 TextUnit
#[async_trait]
pub trait LlmCapability: Send + Sync {
    async fn embed(&self, units: &[TextUnit]) -> Result<Vec<Vec<f32>>>;
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<(TextUnit, f32)>>;
    async fn complete(&self, unit: &TextUnit, partial: &str) -> Result<String>;
    async fn qa(&self, question: &str, scope_root: Option<&str>) -> Result<QaAnswer>;
    async fn summarize(&self, units: &[TextUnit]) -> Result<String>;
}
```

```
wiki-llm/src/
├── capability.rs  LlmCapability 端口 + TextUnit（领域无关）
├── llm_client.rs  依赖注入的 LlmClient trait（复用 agent-context-db 同名抽象）
├── embed.rs       增量 embedding（diff_embed，输入 TextUnit）
├── search.rs      语义搜索（向量 + BM25 混合，VectorStore 注入）
├── complete.rs    行内补全
├── qa.rs          RAG 问答
├── summarize.rs   摘要
├── rewrite.rs     改写（风格 / 长度 / 语言）
├── tag.rs         自动打标签 / 分类
└── lib.rs
```

> 已删除原 `action.rs` 里的 `LlmDocAction`/`LlmGraphAction` 领域枚举——那是反向依赖领域层的耦合点。领域动作在各自 crate 内定义,统一走 `LlmCapability` 端口。

### RAG 检索的权限过滤（补充 #3，安全红线）

> **越权泄露风险**：原 RAG 管线直接把向量命中的 Block 喂给 LLM,未经权限校验,会把用户无权访问的 Block 内容泄露进答案。检索路径必须内置权限过滤。

```
用户问题 + RequestContext { user_id, roles }
  → wiki-llm::search（向量 + BM25，召回候选 Block）
  → ★ 权限过滤：对每个候选 Block 调 wiki-collab::permission
       PermissionFilter::can_read(user, block) → 剔除无权 Block
  → 过滤后候选构建 context prompt（仅授权内容进 LLM）
  → LlmClient 调用
  → 返回答案 + 引用（引用列表同样只含授权 Block）
```

```rust
/// 检索层权限端口——由 wiki-collab::permission 实现，检索管线注入
#[async_trait]
pub trait PermissionFilter: Send + Sync {
    async fn can_read(&self, ctx: &RequestContext, block_id: &str) -> bool;
    /// 批量过滤，检索热路径用（避免逐条 await）
    async fn filter_readable(&self, ctx: &RequestContext, block_ids: Vec<String>) -> Vec<String>;
}
```

- 过滤发生在 **prompt 构建之前**,无权 Block 绝不进入 LLM 上下文,也不出现在引用溯源里。
- 批量 `filter_readable` 在向量召回后、rerank 前执行,避免逐条 await 拖慢热路径。
- 此端口由 `wiki-collab` 的 `SpaceRole` + Block 级权限实现,检索层只依赖 trait,不 `use` 权限具体类型（遵守 §1.5 单向依赖）。

### Embedding 增量维护（文档）

```
Block 创建 / 更新
  → wiki-llm::embed::diff_embed()
      仅重算变更 Block 及其父路径（其余 Block embedding 不变）
  → upsert → VectorStore（注入实例，collection: "wiki_blocks"）
  → 更新 Block.embedding 字段 + embedding_version（见 §15.3 陈旧检测）
```

---

## 8. LLM Wiki 工作流

> 核心理念（来自 Andrej Karpathy，2026.04）：LLM 是 wiki 的**全职编辑**，不是临时检索器。
> 每次 Ingest 让 wiki 更丰富，每次 Query 的好答案反写回 wiki，Lint 保持 wiki 健康——知识随时间复利增长。

### 8.1 三操作管线总览

```
raw/（原始资料目录）
  │
  ▼  Ingest
wiki/（结构化知识库）
  │
  ▼  Query ──→ 答案 ──→ 反写 wiki（可选）
  │
  ▼  Lint（定期）
     修复矛盾 / 孤页 / 缺页
```

### 8.2 Ingest — 原料转知识

将外部原始资料（文档、URL、PDF、代码库）消化进 wiki，LLM 自动更新相关页面并建立交叉链接。

**单次 Ingest 流程：**

```
1. 原料放入 raw/（Markdown / PDF / URL / 代码片段）
2. IngestPipeline::run(source) 触发：
   a. LLM 读取原料，与已有 wiki 摘要对比（避免重复录入）
   b. LLM 写 summary 页（raw/ 下资料的摘要文档）
   c. LLM 识别涉及的 entity / concept 页面（可能触动 10~15 个页面）
   d. 对每个相关页面：追加新知识、更新矛盾标注、添加反向链接
   e. 更新 index 页（wiki 目录）
   f. 写入 ingest_log（时间戳 + 原料 ID + 触动页面列表）
3. 所有写操作走 Op 日志，触发 wiki.block.updated 事件 → 增量 embedding
```

**数据结构：**

```rust
pub struct IngestSource {
    pub id: SourceId,                    // UUID v7
    pub kind: SourceKind,                // Markdown | Url | Pdf | CodeSnippet | Text
    pub content: String,                 // 原始内容（或 URL）
    pub meta: HashMap<String, String>,   // 来源元数据（标题 / 作者 / 日期）
}

pub struct IngestResult {
    pub source_id: SourceId,
    pub summary_doc_id: DocId,           // 生成的摘要文档
    pub touched_docs: Vec<DocId>,        // 被更新的 wiki 页面
    pub new_docs: Vec<DocId>,            // 新建的 wiki 页面
    pub contradictions: Vec<Contradiction>, // 发现的矛盾
    pub log_entry: IngestLogEntry,
}

pub struct Contradiction {
    pub doc_id: DocId,
    pub block_id: BlockId,
    pub existing: String,    // 已有说法
    pub incoming: String,    // 新来源的说法
    pub severity: ContradictionSeverity, // Minor | Major | Critical
}
```

**Ingest Prompt 策略：**

```
系统角色：你是 wiki 的编辑，负责将新知识整合进已有知识库。
任务：
  1. 阅读新原料，提取关键 entity / concept / claim
  2. 对照已有相关页面的摘要，判断：
     - 新增内容（直接追加）
     - 更新内容（修改已有说法，标注版本）
     - 矛盾内容（标注 contradiction，不强行覆盖）
     - 需要新建页面（entity / concept 尚无对应页）
  3. 为每个被触动页面输出 Op 列表（InsertBlock / UpdateBlock）
  4. 更新 index 页中的条目
输出格式：结构化 JSON（OpBatch + IngestResult），不输出自由文本
```

**模块位置：**

```
wiki-llm/src/
└── workflow/
    ├── ingest.rs      IngestPipeline / IngestSource / IngestResult
    ├── source.rs      SourceKind / SourceLoader（URL fetch / PDF parse）
    └── log.rs         IngestLog / IngestLogEntry
```

### 8.3 Query — 问答与知识反写

Query 不是普通 RAG——好的答案会反写回 wiki，让知识库持续增长。

**Query 流程：**

```
1. 用户/Agent 提问
2. wiki-llm::search 混合检索（向量 + BM25）→ top-k Block 片段
3. LLM 基于已合成的 wiki 内容回答（不读原始 raw/）
4. 返回答案 + 引用 BlockId 列表
5. 【可选】反写判断：
   - 如果答案推导出新知识（非 wiki 中已有内容）
   - 调用 WriteBackPolicy::evaluate(answer, cited_blocks)
   - 通过 → 生成新 Block 追加到对应页面或创建新页面
   - 拒绝 → 仅返回答案，不写入
```

**数据结构：**

```rust
pub struct QueryResult {
    pub answer: String,
    pub cited_blocks: Vec<(BlockId, f32)>,  // BlockId + 相关度评分
    pub write_back: Option<WriteBackResult>,
}

pub struct WriteBackResult {
    pub target_doc_id: DocId,
    pub new_block_id: BlockId,
    pub reason: String,   // LLM 说明为何值得写回
}

pub enum WriteBackPolicy {
    /// 永不自动写回（人工审核后手动触发）
    Never,
    /// LLM 判断后自动写回（高置信度才触发）
    Auto { confidence_threshold: f32 },
    /// 写回前询问调用方（Agent / 用户）
    AskFirst,
}
```

**模块位置：**

```
wiki-llm/src/
└── workflow/
    ├── query.rs       QueryPipeline / QueryResult / WriteBackPolicy
    └── write_back.rs  WriteBackEvaluator（判断答案是否值得写回）
```

### 8.4 Lint — 定期知识审计

Lint 是 wiki 的健康检查，发现并修复知识库中的结构性问题。

**Lint 检查项：**

| 检查类型 | 说明 | 自动修复 |
|---|---|---|
| **矛盾检测** | 不同页面对同一 entity 的描述相互矛盾 | 否（标注，人工决策） |
| **孤页检测** | 页面无任何入链（referenced_by 为空） | 可选（建议删除或合并） |
| **缺页检测** | 页面中提到某 entity/concept 但无对应页面 | 可自动创建空白页占位 |
| **过时检测** | 页面内容与最新 Ingest 记录明显冲突 | 否（标注 stale） |
| **重复检测** | 两个页面内容高度相似（embedding 余弦距离 < 阈值） | 建议合并 |
| **断链检测** | references 字段中的 DocId 已不存在 | 自动清理断链 |

**Lint 流程：**

```
1. WikiLinter::run(space_id) 触发（可定时 / 手动）
2. 全量扫描（走倒排索引，非全表扫描）：
   a. 收集所有 referenced_by 为空的页面 → 孤页列表
   b. 提取全库 entity mention → 对照已有页面标题 → 缺页列表
   c. 向量聚类 → 余弦距离 < threshold 的页面对 → 重复候选
   d. 清理 references 中的断链
   e. LLM 抽样检查高优先级页面是否存在语义矛盾
3. 生成 LintReport
4. 发布 wiki.lint.completed 事件（携带 LintReport）
5. 调用方决定如何处理（自动修复 / 展示给用户 / 忽略）
```

**数据结构：**

```rust
pub struct LintReport {
    pub space_id: SpaceId,
    pub ran_at: DateTime<Utc>,
    pub orphan_pages: Vec<DocId>,
    pub missing_pages: Vec<String>,           // entity 名称，尚无对应页
    pub contradictions: Vec<Contradiction>,
    pub stale_pages: Vec<(DocId, String)>,    // (DocId, 原因)
    pub duplicate_candidates: Vec<(DocId, DocId, f32)>, // (a, b, 相似度)
    pub broken_links_fixed: usize,
}

pub struct LintConfig {
    pub duplicate_threshold: f32,   // 默认 0.92（余弦相似度）
    pub stale_check_llm: bool,      // 是否用 LLM 检查语义过时
    pub auto_fix_broken_links: bool,
    pub auto_create_missing_pages: bool,
}
```

**模块位置：**

```
wiki-llm/src/
└── workflow/
    ├── lint.rs        WikiLinter / LintReport / LintConfig
    └── dedup.rs       重复页面检测与合并建议
```

### 8.5 工作流事件

| 事件主题 | 发布方 | 说明 |
|---|---|---|
| `wiki.ingest.completed` | IngestPipeline | 携带 IngestResult，触动页面已更新 |
| `wiki.ingest.contradiction` | IngestPipeline | 发现矛盾，待人工决策 |
| `wiki.query.write_back` | QueryPipeline | 答案反写触发，携带 WriteBackResult |
| `wiki.lint.completed` | WikiLinter | 携带 LintReport |
| `wiki.lint.missing_page` | WikiLinter | 发现缺页，可触发自动创建 |

### 8.6 wiki-workflow 完整模块结构

```
wiki-llm/src/
└── workflow/
    ├── mod.rs         pub use ingest / query / lint
    ├── ingest.rs      IngestPipeline / IngestSource / IngestResult
    ├── source.rs      SourceKind / SourceLoader
    ├── log.rs         IngestLog / IngestLogEntry
    ├── query.rs       QueryPipeline / QueryResult / WriteBackPolicy
    ├── write_back.rs  WriteBackEvaluator
    ├── lint.rs        WikiLinter / LintReport / LintConfig
    └── dedup.rs       重复检测
```

**Feature gate：**

```toml
[features]
llm-workflow = ["wiki-llm/workflow", "dep:tokio"]
```

---

## 9. 协作层 wiki-collab

基于 `agent-crdt`，文档/图共用同一协作协议。

**定位说明**：`agent-crdt` 是**合并逻辑层**，不持有存储——它在内存中执行 CRDT 合并计算（无冲突），合并后的 Block 树状态和 Op 序列均由 `WikiStorage` 接口持久化到 DB（PG）。DB 是唯一真相源，`agent-crdt` 只是合并算子。

```
wiki-collab/src/
├── session.rs     CollabSession（连接 / 心跳 / 离线 Op 队列）
├── awareness.rs   光标位置 / 在线状态广播
├── permission.rs  SpaceRole（Owner / Editor / Viewer）+ Block / Node 级权限
├── sync.rs        Op 队列合并 + CRDT 合并计算（agent-crdt）
└── lib.rs
```

### 协作流程

```
Client A 提交 Op（文档或图均适用）
  → wiki-collab::sync 权限校验
  → agent-crdt 内存合并（CRDT 合并算子，无冲突，不持久化）
  → 合并后 Block 树状态写 WikiStorage::doc_store()（PG blocks 表）
  → Op 序列化后写 WikiStorage::op_log()（PG op_log 表，用于离线回放）
  → 广播 Delta 给同 Doc/Graph 所有在线 Client（uwu_event_mesh）
  → 离线 Client 重连 → 从 WikiStorage::op_log() 拉取 Op 队列回放
```

**DB 是存储层，agent-crdt 是合并计算层，两者职责分离：**

| 职责 | 承担方 |
|---|---|
| CRDT 合并计算（无冲突合并算子） | `agent-crdt`（内存，无 I/O） |
| 合并后状态持久化 | `WikiStorage::doc_store()` → PG blocks 表 |
| Op 日志持久化（离线回放） | `WikiStorage::op_log()` → PG op_log 表 |
| Op 实时广播 | `uwu_event_mesh`（wiki.collab.op 事件） |

---

## 10. 存储层外部注入

> **v2.1 破坏性变更**：uwu_wiki 不再自持存储实例。所有 VectorStore / 持久化后端由外部调用方在初始化时注入。
> 动机：uwu_wiki 作为 `agent-context-db` 的 wiki 子域使用时，由 context-db 统一持有存储（PG + Qdrant），避免双存储冲突。

### 10.1 注入接口

```rust
/// 调用方实现此 trait 并在初始化时传入
pub trait WikiStorage: Send + Sync + 'static {
    /// Block 向量存储（embedding upsert / 语义检索）
    fn vector_store(&self) -> Arc<dyn VectorStore>;
    /// 文档/Block 持久化（get / save / delete / list）
    fn doc_store(&self) -> Arc<dyn DocStore>;
    /// Op 日志持久化（append / replay）
    fn op_log(&self) -> Arc<dyn OpLog>;
    /// 全文倒排索引（精确关键词检索，补充 #1）
    fn text_index(&self) -> Arc<dyn TextIndex>;
    /// 引用图持久化（backlinks，补充 #2）
    fn link_store(&self) -> Arc<dyn LinkStore>;
    /// 二进制附件存储（补充 #4）
    fn blob_store(&self) -> Arc<dyn BlobStore>;
    /// 文档版本快照（历史浏览 / diff / 回滚，补充 #5）
    fn version_store(&self) -> Arc<dyn DocVersionStore>;
}

/// 端口：全文倒排索引（#1）——正文精确关键词/短语检索，补语义检索之短
#[async_trait]
pub trait TextIndex: Send + Sync {
    async fn index_block(&self, block_id: &str, text: &str, meta: serde_json::Value) -> Result<()>;
    /// 精确/前缀/短语查询，返回 BlockId + 命中片段
    async fn search(&self, query: &TextQuery, top_k: usize) -> Result<Vec<TextHit>>;
    async fn remove(&self, block_id: &str) -> Result<()>;
}

/// 端口：引用图持久化（#2）——承载 §4.1 LinkGraph
#[async_trait]
pub trait LinkStore: Send + Sync {
    async fn upsert_links(&self, from: &str, links: &[WikiLink]) -> Result<()>;
    async fn backlinks(&self, target: &str) -> Result<Vec<WikiLink>>;
    async fn broken_links(&self) -> Result<Vec<WikiLink>>;
}

/// 端口：二进制附件（#4）——图片/文件 blob，带引用计数 GC
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, bytes: Vec<u8>, content_type: &str) -> Result<BlobId>;
    async fn get(&self, id: &BlobId) -> Result<(Vec<u8>, String)>;
    /// Block 引用变化时更新引用计数；归零由 GC 异步回收
    async fn ref_delta(&self, id: &BlobId, delta: i32) -> Result<u64>;
    async fn gc(&self) -> Result<usize>;   // 回收 refcount=0 的孤儿 blob
}

/// 端口：文档版本快照（#5）——用户级历史浏览/比较/回滚
/// 注意：与 agent-context-db MVCC 分层——此为 wiki 领域版本，
/// OpLog 是 CRDT 回放用，二者不同。version_store 存的是可读版本快照。
#[async_trait]
pub trait DocVersionStore: Send + Sync {
    /// 提交一次快照（Op 累积到阈值 / 手动保存点时触发）
    async fn snapshot(&self, doc_id: &str, doc: &Document, label: Option<String>) -> Result<VersionId>;
    async fn list_versions(&self, doc_id: &str) -> Result<Vec<VersionEntry>>;
    async fn get_version(&self, doc_id: &str, v: VersionId) -> Result<Document>;
    /// 结构化 diff（Block 级增/删/改）
    async fn diff(&self, doc_id: &str, a: VersionId, b: VersionId) -> Result<DocDiff>;
    async fn restore(&self, doc_id: &str, v: VersionId) -> Result<()>;   // 回滚=以旧版为内容提交新版
}

/// WikiSpace 初始化时注入存储
pub struct WikiSpace {
    pub id: SpaceId,
    storage: Arc<dyn WikiStorage>,
    // ...
}

impl WikiSpace {
    pub fn new(id: SpaceId, storage: Arc<dyn WikiStorage>) -> Self { ... }
}
```

> **端口扩展说明**：新增的 4 个端口(`TextIndex`/`LinkStore`/`BlobStore`/`DocVersionStore`)与原有 3 个一样,只是 trait 定义在 `wiki-core`,实现由宿主注入。`agent-context-db` 的 `ContextDbWikiStorage` 复用其 PG(text_index 走 PG 全文索引 tsvector / blob 走 AGFS 内容层 / version 走 MVCC / link 走 PG 表)+ Qdrant,不引入新的存储系统。`wiki-testkit` 的内存实现同步提供全部 7 个端口的 HashMap 版。

### 10.2 参考实现移出核心（破坏性,见 §1.5 缺陷 2）

> `wiki-core` 只含 `WikiStorage`/`DocStore`/`OpLog`/`VectorStore` 的 **trait 定义**,不含任何实现,以保持"零依赖纯 Block 引擎"的纯粹性。

`MemoryWikiStorage`(HashMap + 内存向量)移到独立 crate:

```
wiki-testkit/src/           # 独立 crate，仅作 dev-dependency
├── memory_storage.rs   MemoryWikiStorage（测试/开发用内存实现）
└── lib.rs
```

生产环境由 `agent-context-db` 注入真实后端;测试环境依赖 `wiki-testkit`。二者都不进 `wiki-core`。

### 10.3 与 agent-context-db 的集成方式

```
agent-context-db 初始化时：
  1. 持有 PG 连接池（AGFS 内容层）
  2. 持有 Qdrant 客户端（索引层）
  3. 构造 ContextDbWikiStorage（实现 WikiStorage trait）
  4. WikiSpace::new(space_id, Arc::new(ContextDbWikiStorage { pg, qdrant }))
  5. wiki 子域的所有读写均通过注入的 storage 层，
     与 Memory / Resource / Skill 等子域共用同一存储实例

uwu:// URI 映射：
  uwu://agent-x/wiki/{doc_id}/{block_id}
  → context-db FS 范式统一寻址
  → 实际读写委托给 WikiSpace 内部的 WikiStorage 实现
```

### 10.4 VectorStore trait（保持不变）

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, collection: &str, id: &str,
                    vector: Vec<f32>, metadata: serde_json::Value) -> Result<()>;
    async fn search(&self, collection: &str, query: Vec<f32>,
                    top_k: usize, filter: Option<serde_json::Value>)
                    -> Result<Vec<VectorSearchResult>>;
    async fn delete(&self, collection: &str, id: &str) -> Result<()>;
}
```

---

## 11. 事件总线集成

通过 `uwu_event_mesh` 解耦同步写操作与异步副作用（embedding / 广播 / LLM 填充）。

| 事件主题 | 发布方 | 订阅方 | 触发条件 |
|---|---|---|---|
| `wiki.block.updated` | wiki-core | wiki-llm | Block 创建/更新 → 增量 embedding |
| `wiki.graph.node.updated` | wiki-graph | wiki-llm | 节点创建/更新 → 增量 embedding（节点 + 一跳邻居） |
| `wiki.llm.fill_request` | wiki-table | LLM Worker | LLM 列行写入 → 异步填充 |
| `wiki.llm.graph_action` | wiki-graph | LLM Worker | 图 LLM 操作（生成/问答/摘要等） |
| `wiki.collab.op` | wiki-collab | 所有在线 Client | Op 实时广播 |
| `wiki.search.query` | wiki-llm | wiki-llm | 异步搜索请求 |
| `wiki.ingest.completed` | IngestPipeline | 调用方 / Agent | Ingest 完成，携带 IngestResult |
| `wiki.ingest.contradiction` | IngestPipeline | 调用方 | 发现矛盾，待人工决策 |
| `wiki.query.write_back` | QueryPipeline | wiki-core | 答案反写触发 |
| `wiki.lint.completed` | WikiLinter | 调用方 | 携带 LintReport |
| `wiki.lint.missing_page` | WikiLinter | IngestPipeline | 发现缺页，可触发自动创建 |
| `wiki.block.linked` | wiki-core | LinkStore | Block 引用变化 → 更新引用图（backlinks，#2） |
| `wiki.blob.orphaned` | wiki-core | BlobStore GC | Block 删除致附件引用归零 → 待 GC（#4） |
| `wiki.doc.snapshot` | wiki-core | DocVersionStore | Op 累积到阈值/手动保存点 → 版本快照（#5） |
| `wiki.embed.stale` | wiki-llm | LLM Worker | 检索命中 embedding_version 落后 → 触发重算（#8） |

---

## 12. Crate 拆分与 Feature 矩阵

```
pkg/uwu_wiki/
├── Cargo.toml       workspace member，re-export 常用类型
├── wiki-core/       Block / Document / Op / Registry + 全部存储/LLM trait 定义（零依赖，无实现）
├── wiki-testkit/    MemoryWikiStorage 等参考实现（dev-dependency，不进生产）
├── wiki-table/      智能表格（依赖 wiki-core, uwu_visual_script；自适配 TextUnit 调 wiki-llm）
├── wiki-graph/      流程图 / 思维导图（依赖 wiki-core；自适配 TextUnit 调 wiki-llm）
├── wiki-llm/        LLM 横切层（领域无关 LlmCapability + TextUnit；LlmClient 注入，不依赖 agent-core）
├── wiki-workflow/   LLM Wiki 工作流 Ingest/Query/Lint（依赖 wiki-core + wiki-llm 端口）
└── wiki-collab/     CRDT 协作（依赖 wiki-core, agent-crdt, uwu_event_mesh）
```

| Crate | Feature | 说明 |
|---|---|---|
| `wiki-core` | `default` | 纯 Block 引擎 + trait 定义，零外部依赖，**无存储/LLM 实现** |
| `wiki-testkit` | - | 参考内存实现，仅 dev-dependency |
| `wiki-table` | `formula` | 启用 uwu_visual_script 公式引擎 |
| `wiki-table` | `llm-fill` | 启用 LLM 列（依赖 wiki-llm 端口） |
| `wiki-graph` | `llm` | 启用图 LLM 能力（经 TextUnit 适配调 wiki-llm） |
| `wiki-graph` | `llm-qa` | 仅启用图 RAG 问答（llm 的子集） |
| `wiki-graph` | `export` | 启用 SVG / PNG / Mermaid / PlantUML 导出 |
| `wiki-llm` | `hybrid-search` | 向量 + BM25 混合检索 |
| `wiki-llm` | `full-text` | 全文倒排精确检索（#1，TextIndex 端口） |
| `wiki-workflow` | `default` | Ingest / Query / Lint 工作流管线（**通用能力，agent-context-db 直接依赖**） |
| `wiki-core` | `backlinks` | 双向链接引用图（#2，LinkGraph + LinkStore 端口） |
| `wiki-core` | `attachments` | 二进制附件 + 引用计数 GC（#4，BlobStore 端口） |
| `wiki-core` | `versioning` | 文档版本快照/diff/回滚（#5，DocVersionStore 端口） |

---

## 13. 依赖关系图

```
                      wiki-core
              (零外部依赖 + 全部 trait 定义)
             /      |        |        \        \
            /       |        |         \        \
     wiki-table  wiki-graph  wiki-llm  wiki-collab  wiki-workflow
         |          |         |          |            |
   uwu_visual   (适配      LlmClient   agent-crdt   依赖 wiki-llm
     _script    TextUnit   (注入)     uwu_event     端口 + wiki-core
                调 wiki-llm  ↑          _mesh
                端口)     不依赖 agent-core

  wiki-testkit（dev-dependency，MemoryWikiStorage，不进生产依赖）

  WikiStorage / LlmClient trait 均在 wiki-core 定义，外部注入
  例：agent-context-db 实现并注入
  单向依赖：wiki-core ← 各能力 crate ← 调用方；横切层不反向依赖领域层
  uwu_wiki 自身不持有任何存储实例，wiki-llm 不持有 LLM 引擎

  WikiStorage 端口全集（均在 wiki-core 定义，宿主注入实现）：
    vector_store / doc_store / op_log          （原有）
    text_index（#1）/ link_store（#2）
    blob_store（#4）/ version_store（#5）
  检索层额外注入 PermissionFilter（#3，由 wiki-collab 实现）
```

---

## 14. 配置参考

```toml
[wiki]
space_id = "default"

[wiki.llm]
model            = "gpt-4o-mini"
embed_model      = "text-embedding-3-small"
embed_batch_size = 64
search_top_k     = 8
search_hybrid    = true          # 向量 + BM25 混合检索

[wiki.table.llm_fill]
enabled         = true
max_concurrency = 4              # 最多同时处理 4 行 LLM 填充

[wiki.collab]
enabled       = true
op_queue_max  = 10000            # 离线 Op 队列上限

[wiki.storage]
# 不在此配置存储后端；存储实例由外部调用方注入
vector_collection       = "wiki_blocks"
vector_dim              = 1536   # text-embedding-3-small
graph_node_collection   = "wiki_graph_nodes"

[wiki.graph]
default_layout        = "dagre"  # dagre / force / radial / treemap
llm_enabled           = true
search_top_k          = 8
search_context_hops   = 1        # RAG 时拉取命中节点的 N 跳邻居扩展 context
embed_incremental     = true     # 增量 embedding（只算变更节点及一跳邻居）

[wiki.workflow]
enabled                  = true

[wiki.workflow.ingest]
max_touched_docs         = 20    # 单次 Ingest 最多更新页面数
auto_create_pages        = true  # 自动为新 entity 创建空白页

[wiki.workflow.query]
write_back_policy        = "ask_first"   # never | auto | ask_first
write_back_confidence    = 0.85          # auto 模式下的置信度阈值

[wiki.workflow.lint]
enabled                  = true
schedule                 = "0 3 * * *"   # 每天凌晨 3 点
duplicate_threshold      = 0.92
stale_check_llm          = true
auto_fix_broken_links    = true
auto_create_missing_pages = false

[wiki.search]
full_text                = true          # 启用全文倒排精确检索（#1）
fuzzy                    = false         # 是否启用模糊匹配

[wiki.attachments]
enabled                  = true          # 附件支持（#4）
max_blob_mb              = 50
gc_schedule              = "0 4 * * *"   # 每天凌晨 4 点回收孤儿 blob

[wiki.versioning]
enabled                  = true          # 文档版本快照（#5）
snapshot_op_threshold    = 50            # 累积 N 个 Op 自动快照
max_versions_per_doc     = 200           # 超出后合并/丢弃最旧快照

[wiki.embedding]
staleness_check          = true          # embedding 陈旧检测（#8）
```

---

## 15. 检索与知识完整性补充

> 本节补齐 §7 语义检索之外的知识库刚需能力（#1 全文检索、#5 版本浏览、#8 embedding 陈旧检测）。#2 双向链接见 §4.1，#3 权限过滤见 §7，#4 附件见 §10.1 `BlobStore`。

### 15.1 全文精确检索（#1）

语义检索(向量+BM25)对"精确 API 名/错误码/罕见术语"召回弱。`TextIndex` 端口(§10.1)提供正文倒排索引,与语义检索**并联融合**:

```
用户查询
  ├─ 语义路 → VectorStore（向量 + BM25，召回语义相近）
  └─ 精确路 → TextIndex（倒排，召回精确 token / 短语 / 前缀）
  → 结果融合（RRF, Reciprocal Rank Fusion）
  → 权限过滤（§7）→ rerank → 返回
```

```rust
pub struct TextQuery {
    pub terms: Vec<String>,          // 精确词
    pub phrase: Option<String>,      // 短语精确匹配
    pub filter: Option<serde_json::Value>,  // tag/status/space 过滤
    pub mode: MatchMode,             // Exact | Prefix | Fuzzy
}
```

- 融合策略用 RRF,避免向量分与 BM25 分量纲不可比。
- `TextIndex` 后端由宿主注入:`agent-context-db` 用 PG `tsvector`;`wiki-testkit` 用内存倒排表。

### 15.2 文档版本浏览与 diff（#5）

`DocVersionStore` 端口(§10.1)提供用户级版本能力。**与 CRDT OpLog 区分**：OpLog 是协作回放用的操作流,`DocVersionStore` 存的是可读**版本快照**(Block 树完整状态 + label)。

```
版本快照触发：
  ├─ 自动：累积 Op 数达 snapshot_op_threshold（默认 50）
  └─ 手动：用户"保存版本"打 label

版本浏览：list_versions(doc) → [VersionEntry { id, label, author, ts }]
版本比较：diff(doc, v_a, v_b) → DocDiff { added/removed/modified: Vec<BlockChange> }
回滚：restore(doc, v) = 以旧版内容提交为新版（不物理删除中间版本，可追溯）
```

- diff 是 **Block 级结构化差异**(增/删/改),不是文本行 diff,契合 Block 树模型。
- 回滚采用"以旧为新"语义,保留完整历史链,符合审计要求。

#### 与 agent-context-db MVCC 的关系（双实现策略）

wiki 的 `DocVersionStore` 与 context-db 的 DAG 版本系统(context-db §4：commit + branch + tag + ASOF + 内容寻址去重)**能力上是子集关系**——wiki 需要的快照/diff/回滚,context-db DAG 全覆盖且更强(内容寻址避免全量快照、支持分支/merge)。为避免"两套版本系统并存导致双写、双存储、语义漂移",采用**端口 + 双实现**:

- **`DocVersionStore` trait 定义留在 wiki-core**,保住 wiki 独立性;trait 不规定存储实现,也不暴露 context-db 的 `CommitId`/`branch` 具体类型（遵守 §1.5 wiki 不依赖宿主类型）。
- **独立部署**（wiki 不挂 context-db）：`wiki-testkit` 提供线性快照实现,够用。
- **挂 context-db**：由 `ContextDbDocVersionStore` 适配器把 wiki 版本操作翻译成 context-db 的 DAG 操作——`snapshot()` → `commit`、`diff()` → context-db 子树 diff、`restore()` → context-db rollback。wiki 的 `VersionId` 是 context-db `CommitId` 的投影。**真值源唯一(context-db DAG)**,wiki 版本是它的一个受限线性视图,零双写。

> 此处理与 §6.3 pred_error(context-db)、§10 WikiStorage 同构:trait 在核心、真值源在宿主、适配器翻译。

**明确不做分支**：知识库场景只需线性历史 + 回滚 + diff。wiki 的并发协作由 CRDT 实时合并(§9)解决,**不依赖版本分支**。因此 `DocVersionStore` 不加 branch/merge API,context-db 底层的分支能力对 wiki 用户隐藏。若未来确有"草稿分支/正式分支"需求,再让 `DocVersionStore` 向 context-db DAG 靠拢——但当前加分支属过度设计。

### 15.3 Embedding 陈旧检测（#8）

`diff_embed` 是异步的——worker 滞后/失败会导致检索命中过期向量。引入 **embedding 版本戳**:

```rust
pub struct Block {
    // ... 原字段
    pub version: u64,               // Block 内容版本
    pub embedding: Option<Vec<f32>>,
    pub embedding_version: u64,     // ★ 生成该 embedding 时的 Block.version
}
```

- **写入**：Block 更新 `version += 1`,但 `embedding_version` 保持旧值,直到 worker 重算后对齐。
- **检索**：命中 Block 若 `embedding_version < version`,标记为 stale,发 `wiki.embed.stale` 事件触发补算;检索结果仍可返回(旧向量),但附 `stale: true` 供调用方决策是否等待重算。
- **一致性保障**：worker 重算幂等——以 `version` 为幂等键,重复事件不重复写。

这样检索永不因异步滞后而静默命中错误向量,陈旧是**可见且自愈**的。

---
