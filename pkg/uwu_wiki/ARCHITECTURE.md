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

统一覆盖文档 Block、表格行、图节点三类实体，不重复实现。

```
wiki-llm/src/
├── action.rs      LlmDocAction / LlmGraphAction 统一调度入口
├── embed.rs       Block / GraphNode embedding 增量生成（diff_embed）
├── search.rs      语义搜索（向量 + BM25 混合，hybrid_search）
├── complete.rs    行内补全
├── qa.rs          RAG 问答（文档 Block 版本）
├── summarize.rs   文档 / 选区摘要
├── rewrite.rs     改写（风格 / 长度 / 语言）
├── tag.rs         自动打标签 / 分类
└── lib.rs
```

### LlmDocAction

```rust
pub enum LlmDocAction {
    Ask        { question: String, doc_id: Option<DocId> },
    Search     { query: String, top_k: usize },
    Summarize  { scope: DocScope },
    Complete   { block_id: BlockId, partial: String },
    Rewrite    { block_id: BlockId, style: RewriteStyle },
    AutoTag    { doc_id: DocId },
    Embed      { block_ids: Vec<BlockId> },   // 手动触发 embedding
}
```

### 文档 RAG 管道

```
用户问题
  → wiki-llm::search（向量 + BM25 混合，VectorStore 注入）
  → 按相关度排序 Block 片段（top-k，带 BlockId 溯源）
  → 构建 context prompt（文档标题 + Block 路径 + 内容）
  → agent-core LLM 调用
  → 返回答案 + 引用 BlockId 列表（可点击跳转）
```

### Embedding 增量维护（文档）

```
Block 创建 / 更新
  → wiki-llm::embed::diff_embed()
      仅重算变更 Block 及其父路径（其余 Block embedding 不变）
  → upsert → VectorStore（注入实例，collection: "wiki_blocks"）
  → 更新 Block.embedding 字段
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

### 10.2 内置参考实现（仅用于测试/开发）

```
wiki-core/src/storage/
├── memory.rs    MemoryWikiStorage（HashMap + 内存向量，无持久化）
└── mod.rs
```

生产环境不使用内置实现；由 `agent-context-db` 或其他宿主注入真实后端。

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

---

## 12. Crate 拆分与 Feature 矩阵

```
pkg/uwu_wiki/
├── Cargo.toml       workspace member，re-export 常用类型
├── wiki-core/       Block / Document / Op / Registry / MemoryWikiStorage（测试用）
├── wiki-table/      智能表格（依赖 wiki-core, uwu_visual_script）
├── wiki-graph/      流程图 / 思维导图 + 全套 LLM（依赖 wiki-core）
├── wiki-llm/        LLM 横切层 + LLM Wiki 工作流（依赖 wiki-core）
└── wiki-collab/     CRDT 协作（依赖 wiki-core, agent-crdt, uwu_event_mesh）
```

| Crate | Feature | 说明 |
|---|---|---|
| `wiki-core` | `default` | 纯 Block 引擎，零外部依赖 |
| `wiki-core` | `memory-storage` | MemoryWikiStorage（测试/开发用） |
| `wiki-table` | `formula` | 启用 uwu_visual_script 公式引擎 |
| `wiki-table` | `llm-fill` | 启用 LLM 列（依赖 wiki-llm） |
| `wiki-graph` | `llm` | 启用 wiki-graph::llm 全套 LLM 能力 |
| `wiki-graph` | `llm-qa` | 仅启用图 RAG 问答（llm 的子集） |
| `wiki-graph` | `export` | 启用 SVG / PNG / Mermaid / PlantUML 导出 |
| `wiki-llm` | `hybrid-search` | 向量 + BM25 混合检索 |
| `wiki-llm` | `llm-workflow` | 启用 Ingest / Query / Lint 工作流管线（**通用能力，agent-context-db 直接依赖**） |

---

## 13. 依赖关系图

```
                      wiki-core
                    (零外部依赖)
                   /     |      \
                  /      |       \
           wiki-table  wiki-graph  wiki-collab
               |          |           |
        uwu_visual   wiki-llm      agent-crdt
          _script        |        uwu_event_mesh
                    WikiStorage
                      (trait)
                         ↑
                    外部注入（调用方实现）
                    例：agent-context-db

所有模块均可通过 uwu_event_mesh 发布/订阅事件（可选，非强依赖）
uwu_wiki 自身不持有任何存储实例
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
```
