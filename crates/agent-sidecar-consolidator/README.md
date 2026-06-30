# agent-sidecar-consolidator

**独立巩固进程** —— 消费 Episode → LearnTrigger → Guard egress → Memory 持久化。

## 概述

Consolidator 作为独立进程运行，消费 consolidation 通道的 Episode 事件，评估是否触发学习，经过 Guard 安全检查后持久化到 UnifiedMemory。

```
Consolidation 通道
  │
  ├─ 1. 接收 Episode
  ├─ 2. LearnTrigger.evaluate()     → Skip / Consolidate / ExtractSkill
  ├─ 3. McpRemote? → Guard.check_egress()  → 白名单校验
  ├─ 4. Guard.enforce()              → 动作安全检查
  └─ 5. Memory.consolidate()         → 持久化到 UnifiedMemory
```

## 运行

```bash
# Demo 模式（mock channel）
cargo run -p agent-sidecar-consolidator

# NATS 生产模式
cargo run -p agent-sidecar-consolidator --features nats -- --nats nats://localhost:4222 --session "*"
```

输出：
```
[consolidator] starting...
[consolidator] extracted skill: pattern-ep-0 (confidence: 0.9)
[consolidator] consolidated episode ep-1
[consolidator] processed 5 episodes, shutting down
```

## 配置

```rust
struct Config {
    max_tokens: 10_000,
    max_retries: 5,
    egress_allowlist: vec!["safe-server"],
    poll_interval_ms: 1000,
}
```

## 流程

| 步骤 | 说明 |
|---|---|
| Guard 初始化 | NoRmRfRule + TokenBudgetRule + McpWriteAllowlistRule |
| LearnTrigger 初始化 | SignificantError(>0.3) + NewPattern(>0.7) + UserConfirmed |
| 主循环 | mock Episode → evaluate → Guard → consolidate |
| ExtractSkill | McpRemote 需 check_egress 通过 |
| Consolidate | 直接持久化到 Memory |
| Skip | 不学习 |

## 后续集成

- ✅ 接 NATS/JetStream → `uwu_nats_bridge` crate，`run_with_nats()` 方法（feature = "nats"）
- 接 agent-mesh → 消费真实 consolidation 通道（`FlowReceiver::recv_consolidation()`）
- TypeRegistry 反序列化 Episode

## 依赖

- `agent-memory` — UnifiedMemory 持久化
- `agent-learning` — LearnTrigger + Episode
- `agent-guard` — GuardLayer 安全检查
- `agent-types-core` + `agent-state` — 基础类型

## License

与仓库一致。
