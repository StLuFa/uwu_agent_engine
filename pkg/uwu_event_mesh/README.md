# uwu_event_mesh

进程内 **事件网格 (Event Mesh)**：层级 topic、因果信封、类型化事件集、可插拔持久化与回放、可配背压、跨网格桥接。

跨进程安全支持：`SerializedEnvelope` + `TypeRegistry`（类型安全反序列化） + `FlowHandle`（四路通道：main / consolidation / monitoring / system） + `sequence_number` / `replay_id`（Crash Recovery 基础）。

FlowMind 双网格架构（Event Mesh + Matrix Mesh）的底层 pub/sub 基座。

## 特性

### 进程内事件网格
- **层级 topic** — `flow.order.created`，支持 `*`（单段）、`>`（多段尾）通配
- **Envelope** — 事件信封带 `id` / `parent_id` / `root_id` / `trace_id` / `idempotency_key` / `ttl` / `headers`，原生支持因果链与去重
- **EventSet（事件集）** — 类型安全的 publish/subscribe，编译期校验 payload 类型
- **持久化 + 回放** — `EventStore` trait 可插拔；自带 `MemoryStore`、`JsonlStore`、`SegmentedStore`
- **WAL group commit** — JsonlStore 后台 writer 批量合并写入与 flush
- **二级索引（topic + 时间）** — `(topic, ts) → (offset, len)` 内存索引，replay 直接 seek，支持时间范围裁剪
- **崩溃恢复** — 记录带 CRC32，开启时校验并截断尾部坏数据；优雅关闭 fsync log + idx
- **段式持久化** — `SegmentedStore` 按大小自动滚动到新段文件，便于归档/清理
- **背压可配** — 每个订阅独立选择 `Block` / `DropNewest` / `DropOldest` / `Disconnect`
- **真·DropOldest 语义** — 自定义环形通道，满时丢队首旧事件以容纳新事件
- **跨网格桥接** — `Bridge` trait + `ChannelBridge`，把多个 mesh 联起来（NATS/Redis 后端只需实现 trait）
- **服务端 Filter** — `Filter::header / pointer_eq / predicate / and / or / not`，订阅声明式裁剪，慢消费者不浪费 buffer
- **批量 Pull** — `Subscription::poll(max, timeout)`，一次拉一批喂给批处理器
- **Consumer Group** — `SubscribeOptions::group(name)`，同 group 内多 sub 竞争消费（一条只投一个），支持 `RoundRobin` / `KeyHash`（粘连同 key）
- **Ack / Redelivery / DLQ** — `AckMode::explicit().with_visibility(..).with_max_attempts(..).with_dlq(..)`，超时未 ack 自动重投，超阈值进死信 topic
- **效果一致幂等消费** — `IdempotencyStore` trait + `process_idempotent` 驱动器，at-least-once + 处理端去重 = effectively-once
- **fan-out 零拷贝** — 事件以 `Arc<Envelope>` 派发给所有订阅者
- **基于 Tokio 的全异步实现**

### 跨进程安全

- **SerializedEnvelope** — 跨进程安全的序列化信封。`payload_bytes: Vec<u8>` 替代 `serde_json::Value`，构造时即序列化，可穿越 NATS / gRPC / Kafka 进程边界
- **TypeRegistry** — 类型注册表。启动期 `register::<T>()`，边界处 `deserialize()` 校验 `TypeId` → 未知类型直接拒绝，防止反序列化注入攻击
- **FlowHandle** — 四路通道发送端。`main(64)` / `consolidation(256)` / `monitoring(64)` / `system(128)`，自动分配单调 `sequence_number`，correlation_id 贯穿整条因果链
- **FlowReceiver** — 四路通道接收端。`recv_main/consolidation/monitoring/system()` + `recv_any()` 多路复用 + `try_recv_*` 非阻塞轮询 + `into_parts()` 拆分为独立 task
- **sequence_number** — 单调递增序列号（`AtomicU64`，SeqCst），每个 FlowHandle 独立计数，消费者可检测 gap
- **replay_id** — 重放标记。Sidecar 重放历史事件时设置，消费者通过 `is_replay()` 跳过副作用操作
- **correlation_id** — 流程关联 ID。同一逻辑流程的所有事件共享一个 correlation_id，`child_of()` 自动继承
- **EventMetadata** — 生产者元数据（`produced_at` + `producer_id` + `ttl`），附着于每个 SerializedEnvelope
- **TypeId** — `domain.event` 命名空间（如 `state.snapshot`、`task.created`），编译期注册 + 运行时校验

## 安装

```toml
[dependencies]
uwu_event_mesh = { path = "../uwu_event_mesh" }
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## 快速上手

### 基础 pub/sub

```rust
use uwu_event_mesh::prelude::*;
use serde_json::json;

#[tokio::main]
async fn main() {
    let mesh = EventMesh::new();
    let mut sub = mesh.subscribe_str("flow.>").unwrap();

    let topic = Topic::new("flow.order.created").unwrap();
    mesh.emit(&topic, json!({"order_id": 42})).await.unwrap();

    let env = sub.recv().await.unwrap();
    println!("got {}: {}", env.topic, env.payload);
}
```

### 跨进程安全发布（FlowHandle）

```rust
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use uwu_event_mesh::prelude::*;

#[derive(Serialize, Deserialize)]
struct OrderCreated { id: u64, amount: f64 }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 注册类型（启动期一次性）
    let registry = Arc::new(TypeRegistry::new());
    registry.register::<OrderCreated>("order", "created");

    // 2. 创建 FlowHandle（主进程发送端）
    let (flow, mut rx) = FlowHandle::new(
        "session-001".into(),   // correlation_id
        registry.clone(),
        "agent-core",           // producer_id
    );

    // 3. 发布到 main 通道 —— 自动分配 sequence_number
    let type_id = TypeId::new("order", "created");
    flow.publish_main(type_id, "flow.order.created", &OrderCreated { id: 1, amount: 9.9 }).await?;

    // 4. 接收并安全反序列化
    let env = rx.recv_main().await.unwrap();
    assert_eq!(env.sequence_number, 1);
    assert_eq!(env.correlation_id, "session-001");
    assert!(env.replay_id.is_none());

    // 方式 A：已知类型直接反序列化
    let order: OrderCreated = env.deserialize_payload()?;
    // 方式 B：通过 TypeRegistry 安全反序列化
    let any = registry.deserialize(&env.type_id, &env.payload_bytes)?;
    let order: &OrderCreated = any.downcast_ref().unwrap();

    println!("Order #{}: ${}", order.id, order.amount);
    Ok(())
}
```

### 四路通道分别消费（Sidecar 模式）

```rust
use uwu_event_mesh::prelude::*;

#[tokio::main]
async fn main() {
    let registry = std::sync::Arc::new(TypeRegistry::new());
    let (flow, mut rx) = FlowHandle::new("task-42", registry.clone(), "main-process");

    // 发布到不同通道
    let tid = TypeId::new("agent", "decision");
    let payload = serde_json::json!({"action": "click", "target": "#btn"});
    // ... 实际使用时用类型化 publish

    // Sidecar consolidator 只消费 consolidation 通道
    let (mut main_rx, mut cons_rx, mut mon_rx, mut sys_rx) = rx.into_parts();

    // 四个独立 task
    tokio::spawn(async move { while let Some(env) = main_rx.recv().await { /* 主循环 */ } });
    tokio::spawn(async move { while let Some(env) = cons_rx.recv().await { /* 学习巩固 */ } });
    tokio::spawn(async move { while let Some(env) = mon_rx.recv().await { /* 异常监控 */ } });
    tokio::spawn(async move { while let Some(env) = sys_rx.recv().await { /* 系统事件 */ } });
}
```

### 类型安全的事件集

```rust
use serde::{Deserialize, Serialize};
use uwu_event_mesh::prelude::*;

#[derive(Serialize, Deserialize)]
struct OrderCreated { id: u64, amount: f64 }

# async fn _demo() -> Result<(), Box<dyn std::error::Error>> {
let mesh = EventMesh::new();
let set = EventSet::new(&mesh, "flow.order")?;
let created = set.kind::<OrderCreated>("created"); // → topic = flow.order.created

let mut sub = set.subscribe(&created);
set.emit(&created, &OrderCreated { id: 1, amount: 9.9 }).await?;

let (_env, payload) = sub.recv().await.unwrap()?;
assert_eq!(payload.id, 1);
# Ok(()) }
```

### 持久化 + 回放

```rust
use std::sync::Arc;
use uwu_event_mesh::prelude::*;

# async fn _demo() -> Result<(), Box<dyn std::error::Error>> {
let store = Arc::new(JsonlStore::open("./.events.jsonl").await?);
let mesh = EventMesh::with_store(store.clone());

// ... 正常 emit 即落盘 ...

// 回放：仅查询
let history = mesh.replay(ReplayFilter::topic("flow.order.>")?, false).await?;

// 回放：派发给当前订阅者
let _ = mesh.replay(ReplayFilter::all(), true).await?;
# Ok(()) }
```

### 段式持久化（自动滚动）

```rust
use std::sync::Arc;
use uwu_event_mesh::prelude::*;

# async fn _demo() -> Result<(), Box<dyn std::error::Error>> {
let mut opts = SegmentedStoreOptions::default();
opts.max_segment_bytes = 64 * 1024 * 1024; // 单段 64 MiB 后滚动
let store = Arc::new(SegmentedStore::open_with("./.events", opts).await?);
let _mesh = EventMesh::with_store(store);
# Ok(()) }
```

目录结构形如 `./.events/00000001.jsonl`、`./.events/00000002.jsonl`，每段独立带 CRC + 索引；`query()` 会跨所有段并按 topic / 时间窗剪枝。

### 跨网格桥接

```rust
use uwu_event_mesh::prelude::*;

# async fn _demo() {
let mesh_a = EventMesh::new();
let mesh_b = EventMesh::new();
let pair = ChannelBridgePair::new();
mesh_a.attach_bridge(pair.a_to_b.clone());
mesh_b.attach_bridge(pair.b_to_a.clone());

// 把对端入站消息驱动到本地 mesh
let mesh_b_clone = mesh_b.clone();
let mut b_inbox = pair.b_inbox;
tokio::spawn(async move {
    while let Some(env) = b_inbox.recv().await {
        let _ = mesh_b_clone.ingest_remote(env).await;
    }
});
# }
```

任何远程后端（NATS / Redis Streams / WebSocket / gRPC）只需实现 `Bridge` trait 并把入站事件喂给 `EventMesh::ingest_remote`。Envelope id 自带去重，自动避免回环。

### 因果链（自动继承 correlation_id + replay_id）

```rust
use uwu_event_mesh::prelude::*;
use serde_json::json;

let parent_topic = Topic::new("flow.order.created").unwrap();
let parent = Envelope::new(&parent_topic, json!({"id": 1}))
    .with_correlation_id("session-001");

let child_topic = Topic::new("flow.order.shipped").unwrap();
let child = Envelope::child_of(&parent, &child_topic, json!({"id": 1}));

assert_eq!(child.root_id, parent.root_id);
assert_eq!(child.parent_id, Some(parent.id));
// 自动继承
assert_eq!(child.correlation_id.as_deref(), Some("session-001"));
```

### 背压策略

```rust
use uwu_event_mesh::prelude::*;

let mesh = EventMesh::new();
let pat = TopicPattern::new("logs.>").unwrap();

// 慢消费者：满时丢最旧的，最新事件总是能进来
let _sub = mesh.subscribe_with(
    pat,
    SubscribeOptions::default()
        .buffer(1024)
        .policy(BackpressurePolicy::DropOldest),
);
```

策略一览：

| 策略 | 行为 | 适用场景 |
|---|---|---|
| `Block`（默认） | publisher `await` 直到有空间 | 严格不丢事件、允许背压传导 |
| `DropNewest` | 满时丢弃新事件 | 旧的更重要（如 audit log） |
| `DropOldest` | 满时丢队首旧事件，最新一定入队 | 只关心最新状态（如 telemetry） |
| `Disconnect` | 满时移除该订阅者 | 快速失败、避免连锁阻塞 |

### 幂等去重

```rust
use uwu_event_mesh::prelude::*;
use serde_json::json;

# async fn _demo() {
let mesh = EventMesh::new();
let t = Topic::new("x.y").unwrap();

let e1 = Envelope::new(&t, json!({})).with_idempotency_key("k-1");
let e2 = Envelope::new(&t, json!({})).with_idempotency_key("k-1");

assert_eq!(mesh.publish(e1).await.unwrap(), 1);
assert_eq!(mesh.publish(e2).await.unwrap(), 0); // 静默丢弃
# }
```

### 服务端 Filter

```rust
use uwu_event_mesh::prelude::*;

let mesh = EventMesh::new();
let pat = TopicPattern::new("orders.>").unwrap();

// 组合：tenant=acme && payload.amount > 100
let filter = Filter::all_of([
    Filter::header("tenant", "acme"),
    Filter::predicate(|e| e.payload["amount"].as_f64().unwrap_or(0.0) > 100.0),
]);
let _sub = mesh.subscribe_with(pat, SubscribeOptions::default().filter(filter));
```

不匹配的事件**不会进入**该订阅的 channel，慢消费者也不会被无关流量塞满。

### 批量 Pull

```rust
# use uwu_event_mesh::prelude::*;
# use std::time::Duration;
# async fn _demo(sub: &mut Subscription) {
loop {
    let batch = sub.poll(100, Duration::from_millis(50)).await;
    if batch.is_empty() { continue; }
    // 批处理 / 批量写下游
}
# }
```

### Consumer Group

```rust
use uwu_event_mesh::prelude::*;

# async fn _demo() {
let mesh = EventMesh::new();
let pat = TopicPattern::new("tasks.>").unwrap();

// 同 group 内的两个 worker 竞争消费 —— 每条事件只投给一个
let _w1 = mesh.subscribe_with(pat.clone(), SubscribeOptions::default().group("workers"));
let _w2 = mesh.subscribe_with(pat, SubscribeOptions::default()
    .group("workers")
    .group_strategy(GroupStrategy::KeyHash { header_key: "user_id".into() }));
# }
```

`RoundRobin`（默认）均匀分发；`KeyHash` 按 header key 哈希粘连，保证同 key 始终落到同一 member（成员稳定时）。

### Ack / Redelivery / DLQ

```rust
use uwu_event_mesh::prelude::*;
use std::time::Duration;

# async fn _demo() {
let mesh = EventMesh::new();
let pat = TopicPattern::new("jobs.>").unwrap();
let mut sub = mesh.subscribe_with(
    pat,
    SubscribeOptions::default()
        .group("workers")
        .ack(AckMode::explicit()
            .with_visibility(Duration::from_secs(30))
            .with_max_attempts(5)
            .with_dlq("dlq.jobs")),
);

while let Some(env) = sub.recv().await {
    match handle(&env).await {
        Ok(_) => { sub.ack(env.id).ok(); }
        Err(_) => { sub.nack(env.id, Requeue::Delay(Duration::from_secs(2))).ok(); }
    }
}
# async fn handle(_e: &Envelope) -> Result<(), ()> { Ok(()) } }
```

超过 `visibility` 仍未 ack 自动重投；达到 `max_attempts` 后投递到 DLQ topic（payload 包 `original_topic` / `original_id`）。

### Effectively-once（at-least-once + 幂等）

```rust
use std::sync::Arc;
use std::time::Duration;
use uwu_event_mesh::*;

# async fn _demo() {
let mesh = EventMesh::new();
let pat = TopicPattern::new("orders.>").unwrap();
let mut sub = mesh.subscribe_with(
    pat,
    SubscribeOptions::default()
        .group("workers")
        .ack(AckMode::explicit()),
);
let store = Arc::new(MemoryIdempotencyStore::new(100_000));

process_idempotent(
    &mut sub,
    store,
    DedupKey::EnvelopeId,
    100,
    Duration::from_millis(50),
    |env| async move {
        // 真业务处理；返回 Err 则触发 nack 重投
        let _ = env;
        Ok(())
    },
).await;
# }
```

实际持久化用 Redis / SQL 替换 `MemoryIdempotencyStore` 即可（实现 `IdempotencyStore` trait）。

## 概念

### Topic 通配

- `*` 匹配单段：`flow.*.created` → `flow.order.created`、`flow.user.created`
- `>` 匹配多段尾（≥1 段，必须末尾）：`flow.>` → `flow.order`、`flow.order.created`

### Envelope 字段

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | `Uuid` | 事件唯一 id（UUID v4） |
| `topic` | `String` | 投递 topic |
| `timestamp` | `DateTime<Utc>` | 创建时间（UTC） |
| `parent_id` | `Option<Uuid>` | 直接因；`None` 表示根事件 |
| `root_id` | `Uuid` | 因果链根（根事件 = `id`） |
| `trace_id` | `Option<String>` | 跨系统追踪 id（OpenTelemetry 兼容） |
| `idempotency_key` | `Option<String>` | 幂等去重键，配合滑动窗口 |
| `ttl_ms` | `Option<u64>` | 超时丢弃 |
| `source` | `Option<String>` | 生产者标识 |
| `type_id` | `Option<TypeId>` | 事件类型标识（`domain.event`），跨进程反序列化必填 |
| `correlation_id` | `Option<CorrelationId>` | 流程关联 ID，`child_of()` 自动继承 |
| `sequence_number` | `u64` | 单调序列号，`FlowHandle` 自动分配，0 = 未分配 |
| `replay_id` | `Option<ReplayId>` | 重放标记，消费者应跳过副作用 |
| `payload` | `serde_json::Value` | JSON 负载 |
| `headers` | `BTreeMap<String, String>` | 键值元数据 |

### SerializedEnvelope 字段（跨进程安全）

与 `Envelope` 的关键差异：`payload_bytes: Vec<u8>` 替代 `payload: Value`，`type_id` / `correlation_id` 为必填。

| 字段 | 类型 | 说明 |
|---|---|---|
| `type_id` | `TypeId` | 事件类型标识（必填） |
| `id` | `Uuid` | 事件唯一 id |
| `topic` | `String` | 投递 topic |
| `timestamp` | `DateTime<Utc>` | 创建时间（UTC） |
| `parent_id` | `Option<Uuid>` | 直接因 |
| `root_id` | `Uuid` | 因果链根 |
| `trace_id` | `Option<String>` | 跨系统追踪 id |
| `correlation_id` | `CorrelationId` | 流程关联 ID（必填） |
| `sequence_number` | `u64` | 单调序列号 |
| `replay_id` | `Option<ReplayId>` | 重放标记 |
| `idempotency_key` | `Option<String>` | 幂等去重键 |
| `ttl_ms` | `Option<u64>` | 超时丢弃 |
| `source` | `Option<String>` | 生产者标识 |
| `payload_bytes` | `Vec<u8>` | 预序列化的 JSON 字节 |
| `metadata` | `EventMetadata` | 生产者元数据（`produced_at` + `producer_id` + `ttl`） |
| `headers` | `BTreeMap<String, String>` | 键值元数据 |

转换：

```rust
// Envelope → SerializedEnvelope（type_id 必填）
let se = SerializedEnvelope::from_envelope(&env)?;
// SerializedEnvelope → Envelope
let env = se.into_envelope()?;
```

### TypeRegistry（类型注册表）

启动期注册所有跨进程事件类型，边界处安全反序列化：

```rust
let registry = TypeRegistry::new();
registry.register::<StateSnapshot>("state", "snapshot");
registry.register::<TaskCreated>("task", "created");

// 边界校验
match registry.deserialize(&envelope.type_id, &envelope.payload_bytes) {
    Ok(any) => { /* 已知类型，安全 dispatch */ }
    Err(EventMeshError::UnknownType(tid)) => {
        tracing::warn!("rejected unknown type: {tid}");
    }
    Err(e) => { /* 序列化失败 */ }
}
```

### FlowHandle / FlowReceiver（四路通道）

```
┌─────────────┐     ┌──────────────────┐
│  FlowHandle  │────▶│ main (64)        │──▶ 主循环（决策→执行）
│  (Clone)     │────▶│ consolidation    │──▶ Sidecar（LearnNode+Guard）
│              │     │     (256)        │
│  seq: Atomic │────▶│ monitoring (64)  │──▶ Sidecar（异常检测）
│  corr_id     │────▶│ system (128)     │──▶ 心跳 / 配置 / 关闭
└─────────────┘     └──────────────────┘
                              │
                     FlowReceiver
                     (recv_main / recv_consolidation
                      / recv_monitoring / recv_system
                      / recv_any / try_recv_* / into_parts)
```

**频道容量常数**：`MAIN_CAPACITY = 64`、`CONSOLIDATION_CAPACITY = 256`、`MONITORING_CAPACITY = 64`、`SYSTEM_CAPACITY = 128`。

**sequence_number 语义**：
- 每个 `FlowHandle` 独立计数（`AtomicU64`，从 1 开始）
- `publish_*()` 自动分配；`publish_envelope()` 仅在 `seq == 0` 时覆盖
- 消费者可检测 gap → 请求重传（Crash Recovery 协议层）

**replay_id 语义**：
- `Some("batch-001")` → 历史重放事件，消费者**必须**跳过副作用操作
- `None` → 实时事件，正常处理
- 通过 `is_replay()` / `Envelope::is_replay()` 检测

### EventStore trait

```rust
#[async_trait::async_trait]
pub trait EventStore: Send + Sync + 'static {
    async fn append(&self, env: Arc<Envelope>) -> Result<()>;
    async fn append_batch(&self, envs: Vec<Arc<Envelope>>) -> Result<()> { /* default */ }
    async fn query(&self, filter: &ReplayFilter) -> Result<Vec<Arc<Envelope>>>;
    async fn len(&self) -> Result<usize>;
    async fn flush(&self) -> Result<()> { Ok(()) }
    async fn shutdown(&self) -> Result<()> { self.flush().await }
}
```

内置实现：

- **`MemoryStore`** — 环形缓冲，`with_capacity(n)` 控制上限
- **`JsonlStore`** — 单文件 JSONL + 二级索引 `.idx`，CRC32 校验，崩溃恢复，`JsonlStoreOptions`：
  - `batch_size` — group commit 批量大小（默认 256）
  - `channel_capacity` — pending 队列容量（默认 4096）
  - `fsync` — 是否每次 flush 后 fsync（默认 false，吞吐优先）
- **`SegmentedStore`** — 目录式多段文件，按 `max_segment_bytes` 自动滚动；`query` 跨段透明聚合

外部后端（Postgres / Redis Streams / Kafka / S3）只需实现 trait。

### Bridge trait

```rust
#[async_trait::async_trait]
pub trait Bridge: Send + Sync + 'static {
    async fn publish_remote(&self, env: Arc<Envelope>) -> Result<()>;
}
```

`EventMesh::attach_bridge(bridge)` 把所有本地发布转发给远端；远端入站事件经 `EventMesh::ingest_remote(env)` 喂回，跳过 store 与桥转发以避免回环（按 envelope id 去重）。

内置 `ChannelBridge` / `ChannelBridgePair` 用于同进程内联调或拓扑分片。

### ReplayFilter

```rust
ReplayFilter::all()
ReplayFilter::topic("flow.order.>")?
    .with_root(some_uuid)
    .with_since(t0)
    .with_until(t1)
    .with_limit(1000);
```

`topic` 命中二级索引；`since/until` 命中**时间索引**——两者都在 seek 之前完成裁剪，不扫无关数据。

## 目录结构

```
src/
├── lib.rs                  // 顶层 re-exports + prelude
├── core/                   // 核心领域类型
│   ├── error.rs            // EventMeshError
│   ├── envelope.rs         // Envelope（跨进程字段）
│   ├── metadata.rs         // EventMetadata
│   ├── serialized_envelope.rs  // SerializedEnvelope（跨进程安全）
│   ├── topic.rs            // Topic / TopicPattern
│   ├── type_id.rs          // TypeId / CorrelationId / ReplayId
│   └── type_registry.rs    // TypeRegistry（安全反序列化）
├── ext/                    // 扩展能力
│   ├── event_set.rs        // 类型安全事件集（EventSet / EventKind）
│   ├── idempotency.rs      // Effectively-once 消费辅助（IdempotencyStore）
│   └── filter.rs           // 声明式服务端过滤（Filter）
├── mesh/                   // 核心 broker + 四路通道
│   ├── mod.rs
│   ├── broker.rs           // EventMesh / publish / fanout / replay / bridge
│   ├── flow_handle.rs      // FlowHandle + FlowReceiver（四路通道）
│   ├── subscriber.rs       // Subscription / Options / Backpressure / Group / Ack
│   ├── ring.rs             // 真·DropOldest 用的环形通道
│   └── dedup.rs            // O(1) 无分配去重环
├── bridge/                 // 跨进程 / 跨网格桥接
│   ├── mod.rs
│   └── channel.rs
└── store/                  // 持久化与回放
    ├── traits.rs
    ├── filter.rs
    ├── memory.rs
    ├── segmented.rs
    └── jsonl/
```

## 性能笔记

- **Topic 匹配零分配与预编译** — `publish` 时 `env.topic` 只拆分一次（`split`），所有 subscribers 匹配时直接比较段切片 `&[&str]`，避免 O(N) 次冗余分割。
- **O(1) 无分配去重环** — DedupRing 利用 FNV 64位哈希取代字符串拼接，避免百万级 QPS 下的高频分配。
- **fan-out 零拷贝** — 事件以 `Arc<Envelope>` 派发给所有订阅者，负载不拷贝。
- **subscribers 读写锁** — 正常发布只拿 Read 锁；仅发现有 dead sender 或 group 重平衡时短暂升 Write 锁清理。
- **JsonlStore Group Commit** — WAL 后台任务合并 256 条/批刷盘，将单条 fsync 的延迟隐藏到批处理中。
- **多级剪枝回放** — `query` 会同时按时间戳窗和 Topic 二级索引做跳跃式剪枝。
- **FlowHandle 零额外分配** — `sequence_number` 用 AtomicU64 CAS 递增，不持锁；`publish_*()` 只在构造 SerializedEnvelope 时分配一次字节缓冲。

## 架构路线图

| 步骤 | 功能 | 状态 |
|---|---|---|
| 1 | SerializedEnvelope + TypeRegistry（跨进程类型安全） | ✅ 已实现 |
| 2 | FlowHandle 四路通道（main/consolidation/monitoring/system） | ✅ 已实现 |
| 3 | sequence_number + replay_id（Crash Recovery 基础） | ✅ 已实现 |
| 4 | NATS Bridge（`NatsBridge` 实现 `Bridge` trait） | ⬜ 计划中 |
| 5 | Crash Recovery 协议（gap 检测 + 重传 + checkpoint 恢复） | ⬜ 计划中 |

## 运行示例

```bash
cargo run --example basic
```

输出包含：类型化订阅 / 原生订阅 / 因果链 / JSONL 持久化 / 回放重派发 / 跨网格桥接。

## 测试

```bash
cargo test
```

覆盖：topic 匹配、pub/sub、通配、幂等、回放、批量写、索引复用、CRC 崩溃恢复、四种背压、真·DropOldest、跨网格桥接、时间范围裁剪、段式滚动与重启加载、SerializedEnvelope 往返、TypeRegistry 注册/反序列化/拒绝未知类型、FlowHandle 四路通道/序列号单调/recv_any 多路复用/clone 共享。

## License

与仓库一致。
