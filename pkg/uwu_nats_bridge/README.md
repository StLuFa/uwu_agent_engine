# uwu_nats_bridge

NATS/JetStream 桥接 — 将 `uwu_event_mesh` 的四路通道映射到 NATS subjects，实现跨进程事件通信。

## 架构

```
主 Agent 进程                       Sidecar 进程
  NatsPublisher                      NatsSubscriber
    │                                  │
    ├─ agent.{id}.main ──── Core NATS ─┤
    ├─ agent.{id}.consolidation ─ JS ─┤  (JetStream 持久化)
    ├─ agent.{id}.monitoring ─── JS ──┤  (JetStream 持久化)
    └─ agent.{id}.system ─── Core NATS ┘
```

## 类型

| 类型 | 说明 |
|---|---|
| `NatsConfig` | 连接配置（URL / JetStream max_age / 重连策略） |
| `NatsSubjects` | 四通道 → NATS subject 映射 |
| `NatsPublisher` | 镜像 `FlowHandle` API，发布到 NATS/JetStream |
| `NatsSubscriber` | 镜像 `FlowReceiver` API，从 NATS/JetStream 订阅 |

## 使用

### 发布端（主进程）

```rust
use uwu_nats_bridge::{NatsPublisher, NatsSubjects, NatsConfig};

let cfg = NatsConfig::for_agent("nats://localhost:4222", "assistant", "sess-1");
let subjects = NatsSubjects::new("sess-1");
let publisher = NatsPublisher::connect(cfg, subjects).await?;

// JetStream 持久化通道
publisher.publish_consolidation(tid, "consolidate.episode", &episode).await?;
```

### 订阅端（Sidecar）

```rust
use uwu_nats_bridge::{NatsSubscriber, NatsConfig};

let cfg = NatsConfig::for_sidecar("nats://localhost:4222", "consolidator");
let mut sub = NatsSubscriber::connect(cfg, "*").await?;

while let Some(env) = sub.recv_consolidation().await {
    let episode: Episode = serde_json::from_slice(&env.payload_bytes)?;
    // 处理 episode...
}
```

## 通道语义

| 通道 | Transport | 语义 |
|---|---|---|
| Main | Core NATS | 低延迟，at-most-once |
| Consolidation | JetStream | 持久化，可回放，durable consumer |
| Monitoring | JetStream | 持久化，滑动窗口，ephemeral consumer |
| System | Core NATS | 心跳 / 配置 / 优雅关闭 |

## 测试

```bash
cargo test -p uwu_nats_bridge  # 12 passed
```

## 消费者

- `agent-sidecar-consolidator` — feature = "nats"
- `agent-sidecar-monitor` — feature = "nats"
