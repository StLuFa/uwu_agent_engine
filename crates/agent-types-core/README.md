# agent-types-core

全系统共享的基础类型（冻结）。零外部依赖（除 serde/chrono/uuid），所有 Agent crate 均可安全依赖它而不引入循环。

## 类型

| 类型 | 说明 |
|---|---|
| `AgentId` | Agent 全局唯一标识（UUID v4） |
| `Action` | Agent 可执行动作（command + params + timestamp） |
| `ActionParams` | 扁平 key-value 参数（builder 模式） |
| `ActionStatus` | 动作生命周期：Hypothetical → Committed / Reverted |
| `Uncertain<T>` | 带置信度的值（置信度 clamped [0, 1]） |
| `Layer<I, O>` | 通用异步管道层 trait |

## 使用

```rust
use agent_types_core::{Action, ActionParams, AgentId};

let action = Action::new("click", ActionParams::new().with("target", "#btn"));
let agent_id = AgentId::new();
```

## 测试

```bash
cargo test -p agent-types-core  # 22 passed
```

## 消费者

全仓 22 个 crate 依赖此 crate，是最底层的类型基石。
