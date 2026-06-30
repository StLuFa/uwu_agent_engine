# agent-crdt

CRDT (Conflict-free Replicated Data Types) — 多 Agent 共享状态的无冲突合并。

## 类型

| 类型 | 合并策略 | 用途 |
|---|---|---|
| `VectorClock` | entry-wise max | 事件因果序（happens-before / concurrent） |
| `GCounter` | element-wise max | 只增计数（任务总数） |
| `PNCounter` | P + N 两个 GCounter | 增减计数（活跃任务数） |
| `LWWRegister<T>` | max-clock wins | 共享值（配置 / 信任分） |
| `ORSet<T>` | add-wins + tombstones | 共享集合（capabilities / tags） |

## 使用

```rust
use agent_crdt::{GCounter, ORSet, VectorClock};

// GCounter: track task counts
let mut c1 = GCounter::new();
c1.inc("agent-A", 3);
let mut c2 = GCounter::new();
c2.inc("agent-B", 4);
let merged = c1.merge(&c2);
assert_eq!(merged.value(), 7);

// ORSet: add-wins capability registry
let mut set = ORSet::new();
set.add("search".to_string(), "tag-1");
set.remove(&"search".to_string());
set.add("search".to_string(), "tag-2"); // add-wins
assert!(set.contains(&"search".to_string()));

// VectorClock: causal ordering
let mut a = VectorClock::new();
a.increment("X");
let mut b = a.clone();
b.increment("Y");
assert!(a.happens_before(&b));
```

## CRDT 三律

- **幂等**: `c.merge(&c) == c`
- **交换**: `c1.merge(&c2) == c2.merge(&c1)`
- **结合**: `c1.merge(&c2).merge(&c3) == c1.merge(&c2.merge(&c3))`

## 测试

```bash
cargo test -p agent-crdt  # 18 passed
```

## 消费者

- `agent-collaboration` — SharedState (CRDT-backed agent coordination)
