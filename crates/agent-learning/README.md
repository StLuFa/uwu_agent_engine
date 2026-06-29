# agent-learning

Agent **自学习** —— LearnNode 条件触发 + Skill 提取 + 自进化防护。

## 概述

Episode 完成后触发学习评估，根据条件决定是否提取 Skill。5 层自进化防护防止失控。

```
Episode 完成
  │
  ├─ LearnTrigger.evaluate(episode, state)
  │     ├─ SignificantErrorCondition  → pred_error > 阈值 → ExtractSkill
  │     ├─ NewPatternCondition        → 成功 + 高置信度 → ExtractSkill
  │     ├─ UserConfirmedCondition     → 成功 → ConsolidateEpisode
  │     └─ (all skip)                 → Skip
  │
  └─ 决策:
        Skip            → 不学习
        Consolidate     → 巩固到 Memory
        ExtractSkill    → SkillVersion + 沙箱验证
        UpdatePreference → 调整风险/策略
```

## 特性

- **LearnTrigger** — 顺序评估条件，首个命中即触发
- **3 个内置条件** — SignificantError / NewPattern / UserConfirmed
- **LearnDecision** — 4 种决策：Skip / ConsolidateEpisode / ExtractSkill / UpdatePreference
- **SkillVersion** — 版本化 Skill：hash + verified + active + 回滚
- **SkillTarget** — 部署目标：LocalCode / McpRemote / LocalPreference
- **5 层防护** — 版本化 + 沙箱验证 + 回滚 + 人工审批 + 配置开关

## 安装

```toml
[dependencies]
agent-learning = { path = "../agent-learning" }
```

## 快速上手

### 创建 LearnTrigger

```rust
use agent_learning::{
    LearnTrigger,
    conditions::{SignificantErrorCondition, NewPatternCondition, UserConfirmedCondition},
};

let trigger = LearnTrigger::new()
    .with_condition(Box::new(SignificantErrorCondition::new(0.3)))
    .with_condition(Box::new(NewPatternCondition::new(0.7)))
    .with_condition(Box::new(UserConfirmedCondition));
```

### 评估 Episode

```rust
use agent_learning::{Episode, EpisodeOutcome};
use agent_state::AgentState;

let episode = Episode {
    episode_id: "ep-1".into(),
    session_id: "s-1".into(),
    task_id: None,
    state_before: None,
    state_after: None,
    actions_taken: vec!["search".into(), "click".into()],
    outcome: EpisodeOutcome::Success { confidence: 0.9 },
    timestamp: Utc::now(),
};

let state = AgentState::new();
let decision = trigger.evaluate(&episode, &state).await;

match decision {
    LearnDecision::ExtractSkill { skill_name, target, confidence } => {
        println!("Extracting: {skill_name} ({confidence})");
    }
    LearnDecision::ConsolidateEpisode => println!("Consolidating"),
    LearnDecision::Skip => println!("Nothing to learn"),
    _ => {}
}
```

### Skill 版本管理

```rust
use agent_learning::{SkillVersion, SkillTarget};

let mut sv = SkillVersion::new(
    "click-popup",
    SkillTarget::LocalCode { crate_name: "agent-reaction".into() },
    "fn handle_popup() { /* new logic */ }",
    "ep-1",
    0.85,
);

// 沙箱验证
sv.verify();
assert!(sv.verified);

// 问题回滚
sv.deactivate();
assert!(!sv.active);
```

## 核心类型

### LearnTrigger

方法：`new()`, `with_condition(c)`, `evaluate(episode, state)` — 返回第一个非 Skip 决策

### LearnDecision

```rust
pub enum LearnDecision {
    Skip,
    ConsolidateEpisode,
    ExtractSkill { skill_name, target: SkillTarget, confidence: f32 },
    UpdatePreference { field, old_value, new_value },
}
```

### LearnCondition trait

```rust
#[async_trait]
pub trait LearnCondition: Send + Sync {
    async fn should_learn(&self, episode: &Episode, state: &AgentState) -> LearnDecision;
}
```

### SkillVersion

```rust
pub struct SkillVersion {
    pub version_id: String,
    pub skill_name: String,
    pub target: SkillTarget,
    pub hash: String,
    pub verified: bool,
    pub active: bool,
    pub confidence: f32,
    // ...
}
```

方法：`new()`, `verify()`, `deactivate()`

### SkillTarget

```rust
pub enum SkillTarget {
    LocalCode { crate_name: String },
    McpRemote { server_id, tool_name, endpoint: String },
    LocalPreference,
}
```

## 内置条件

| 条件 | 触发规则 | 决策 |
|---|---|---|
| `SignificantErrorCondition(0.3)` | `pred_error > 0.3` | ExtractSkill |
| `NewPatternCondition(0.7)` | Success + confidence ≥ 0.7 | ExtractSkill |
| `UserConfirmedCondition` | Success | ConsolidateEpisode |

## 目录结构

```
src/
├── lib.rs           // Episode + EpisodeOutcome
├── trigger.rs       // LearnCondition trait + LearnDecision + LearnTrigger + tests
├── skill.rs         // SkillTarget + SkillVersion + tests
└── conditions/
    └── mod.rs       // 3 个条件实现 + tests
```

## 测试

```bash
cargo test -p agent-learning
```

覆盖：条件优先级短路、全部 Skip、Skill 版本生命周期、pred_error 阈值触发/跳过、新模式高置信度触发、用户确认巩固。

## 依赖

- `agent-state` — AgentState (pred_error)
- `agent-types-core` — 基础类型
- `async-trait` — async trait
- `serde` + `chrono` + `uuid` — 序列化

## License

与仓库一致。
