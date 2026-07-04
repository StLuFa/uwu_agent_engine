# agent-context-db

通用核心之上叠加 uwu 五维深度耦合 + LlmClient 实现 + 创新功能。

## 模块

| 模块 | 内容 |
|------|------|
| `state_bridge` | `StateBridge` — load/checkpoint/fork/promote/discard，依赖 VersionStore |
| `metacog_bridge` | `MetacogBridge` — log_pred_error 冷归档 + retrieve_calibration 冷热合并 |
| `character_constraint` | `CharacterConstraint` — 关键词写入前置校验 |
| `llm` | `HttpLlmClient`(OpenAI 兼容 API) + `MockLlmClient`(确定性测试响应) |
| `sandbox` | `WriteGate`(F25 安全沙箱) — 关键词+LLM 双层闸门 |
| `innovation` | `FederationProtocol`(F18 联邦) + `MultimodalAligner`(F29 多模态对齐) |
| `mesh_bridge` | `ReactionLearner`(U10) + `EventMeshBridge`(U11) |
| `wasm` | `WasmSandbox`(U12) — 统计/聚类/趋势/自定义模块 |

## StateBridge

```rust
let bridge = StateBridge::new(store, versions);
bridge.checkpoint("a1", StateScope::Mid, &snap, tenant).await?;
let fork = bridge.fork("a1", StateScope::Mid).await?;
bridge.promote_fork(&fork, MergeStrategy::FastForward).await?;
```

## MetacogBridge

```rust
bridge.log_pred_error("a1", &sample, tenant).await?;
let results = bridge.retrieve_calibration("a1", window, &hot_samples).await?;
```

## WriteGate

```rust
let gate = WriteGate::new(keyword_constraint, semantic_sandbox);
let verdict = gate.gate(&entry).await?; // Pass / Reject / Quarantine
```

## 依赖

`context-db-core` / `context-db-retrieve` / `context-db-version` + `serde` / `reqwest` / `parking_lot`。
