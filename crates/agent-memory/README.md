# agent-memory

Agent **统一记忆** —— 一个向量空间 + 四个查询视图。

## 概述

统一记忆将 Agent 的记忆建模为四种类型的查询视图，底层共享同一个向量嵌入空间 + 元数据索引。

```
              ┌─────────────────────────┐
              │    UnifiedMemory         │
              │  (向量嵌入 + 元数据索引)   │
              └──────────┬──────────────┘
           ┌─────────────┼─────────────┐
           ▼             ▼             ▼
     Episodic       Semantic      Procedural      Working
     (情景记忆)      (语义记忆)     (程序记忆)      (工作记忆)
     "what happened" "what I know" "how to do"   "what's now"
```

作为 visual_script NodeDefinition 注册：`"memory.retrieve"`（Impure + Async）。

## 特性

- **四型记忆** — Episodic/Semantic/Procedural/Working 作为查询视图
- **向量检索** — 基于余弦相似度的语义检索，支持类型过滤 + 阈值
- **Episode 巩固** — 将交互回合提取为语义/程序/情景记忆
- **State/Persona 持久化** — `persist_state()` / `persist_persona()` 快照存入记忆
- **Mock Embedding** — 开发用确定性伪嵌入，无需外部 API
- **MemoryScore** — 三维评分（相似度 + 时效性 + 频率）
- **MemoryFacade** — 便捷门面，封装常用操作

## 安装

```toml
[dependencies]
agent-memory = { path = "../agent-memory" }
```

## 快速上手

### 存入和检索记忆

```rust
use agent_memory::{UnifiedMemory, Memory, MemoryType, Embedding, RetrievalIntent};

let mut um = UnifiedMemory::new(16); // 16 维嵌入

// 存入语义记忆
let emb = Embedding::mock("rust async programming guide", 16).values;
um.upsert(Memory::new(
    MemoryType::Semantic,
    "rust async programming guide",
    emb,
));

// 检索
let intent = RetrievalIntent::simple("async rust");
let results = um.retrieve(&intent);
for m in &results {
    println!("{} (score: {:.2})", m.content, m.score.total);
}
```

### 按类型过滤

```rust
let results = um.retrieve_typed(
    &RetrievalIntent::simple("click button"),
    Some(vec![MemoryType::Procedural]),  // 只查程序记忆
);
```

### Episode 巩固

```rust
use agent_memory::Episode;

let episode = Episode::new("agent-1", "find user data", "found 5 records", true)
    .with_action("search database")
    .with_action("filter by date")
    .with_observation("5 rows returned");

um.consolidate_episode(&episode);

// → 自动生成 Episodic + Procedural 记忆
assert!(um.count_by_type(MemoryType::Episodic) >= 1);
assert!(um.count_by_type(MemoryType::Procedural) >= 1);
```

### State/Persona 持久化

```rust
// State 快照 → Working 记忆
um.persist_state("agent-1", r#"{"version":5, "facts":[...]}"#);

// Persona 快照 → Semantic 记忆
um.persist_persona("agent-1", r#"{"identity":{"name":"Alice"}}"#);
```

### MemoryFacade 便捷接口

```rust
use agent_memory::MemoryFacade;

let mut facade = MemoryFacade::new(16);

// 快速检索
let result = facade.retrieve("database query");
println!("found {} memories", result.items.len());

// 快速持久化
facade.persist_state("agent-1", r#"{"version":1}"#);
```

### 自定义嵌入

```rust
// 生产环境：调用 OpenAI text-embedding-3-small 等
// 开发环境：使用 Embedding::mock()
let real_emb = Embedding::new(vec![0.1, -0.3, 0.8, /* ... 1536 dims */]);

let memory = Memory::new(
    MemoryType::Semantic,
    "knowledge entry",
    real_emb.values,
);
```

## 核心类型

### Memory

```rust
pub struct Memory {
    pub id: String,
    pub memory_type: MemoryType,          // Episodic/Semantic/Procedural/Working
    pub content: String,                  // 文本内容
    pub embedding: Vec<f32>,              // 向量嵌入
    pub score: MemoryScore,               // 综合评分
    pub state_snapshot_json: Option<String>,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u32,
}
```

### MemoryType

```rust
pub enum MemoryType {
    Episodic,   // 具体经历/事件
    Semantic,   // 事实/知识
    Procedural, // 技能/流程
    Working,    // 当前上下文
}
```

### MemoryScore

```rust
pub struct MemoryScore {
    pub similarity: f32,  // 向量相似度
    pub recency: f32,     // 时效性
    pub frequency: f32,   // 访问频率
    pub total: f32,       // 综合评分 = avg(三路)
}
```

### UnifiedMemory

```rust
pub struct UnifiedMemory { /* HashMap<String, Memory> */ }
```

方法：`upsert()`, `upsert_batch()`, `retrieve(intent)`, `retrieve_typed(intent, types)`, `persist_state()`, `persist_persona()`, `consolidate_episode()`, `count_by_type()`, `len()`

### RetrievalIntent

```rust
pub struct RetrievalIntent {
    pub query: String,
    pub preferred_types: Option<Vec<MemoryType>>,
    pub max_results: usize,
    pub min_similarity: f32,
}
```

方法：`simple(query)`, `typed(query, types)`, `with_max(n)`, `with_threshold(t)`

### Episode

```rust
pub struct Episode {
    pub episode_id: String,
    pub agent_id: String,
    pub goal: String,
    pub actions: Vec<String>,
    pub observations: Vec<String>,
    pub outcome: String,
    pub success: bool,
    pub extracted_insights: Vec<String>,
    pub occurred_at: DateTime<Utc>,
}
```

## 巩固策略

`consolidate_episode()` 的提取逻辑：

| Episode 字段 | 产出的 Memory |
|---|---|
| 整体回合 | → Episodic 记忆（结果 + 全部动作） |
| `extracted_insights` | → Semantic 记忆（每条洞察一条） |
| success + actions | → Procedural 记忆（动作序列作为流程） |

## 目录结构

```
src/
├── lib.rs          // MemoryFacade + RetrievedMemories + tests
├── types.rs        // MemoryType + Memory + MemoryScore
├── embedding.rs    // Embedding + cosine_similarity + mock()
├── retrieve.rs     // RetrievalIntent
├── consolidate.rs  // Episode + consolidate_episode()
└── unified.rs      // UnifiedMemory + tests
```

## 后续集成

- 接 `uwu_database::VectorStore` → 生产级向量检索（Qdrant/Pgvector/LanceDB）
- 接 `uwu_database::Database` → PostgreSQL 元数据查询
- 接外部 embedding 服务 → OpenAI/本地模型替代 mock

## 测试

```bash
cargo test -p agent-memory
```

覆盖：余弦相似度、mock 嵌入确定性、retrieve 排序、retrieve_typed 过滤、persist_state 往返、Episode 巩固生成多类型记忆、MemoryFacade 便捷接口、空结果处理。

## 依赖

- `agent-state` — StateSnapshot
- `agent-types-core` — 基础类型
- `serde` + `serde_json` + `chrono` + `uuid` — 序列化与标识
- `async-trait` + `tokio` — async trait + 运行时

## License

与仓库一致。
