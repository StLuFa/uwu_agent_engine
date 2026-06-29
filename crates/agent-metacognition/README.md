# agent-metacognition

Agent **元认知维度** —— 三信号在线自校准 + TTS（Time To Stop）渐进式预算控制。

## 概述

Metacognition 在每步决策后评估"这一步对吗？"，融合三路独立信号给出 `MetaAction` 建议：

```
每步决策后:
  Metacognition.evaluate(state, decision)
  ├── Proceed             → 正常放行
  ├── RetryDecision       → 回滚 State，重新推理
  ├── RequestClarification → 暂停，向用户提问
  ├── SwitchStrategy      → 切换推理模式（检测到循环/漂移）
  ├── DelegateToHuman     → 升级给人类
  └── AbortOnBudget       → 预算耗尽，终止
```

### 三信号融合

```
meta_score = w1 × verifier + w2 × (1 - pred_error) + w3 × cost_remaining
w1=0.5, w2=0.3, w3=0.2（可配置）
```

| 信号 | 来源 | 成本 |
|---|---|---|
| verifier | 本地校准模型（Qwen2.5-0.5B 或 mock） | ~50ms |
| pred_error | State fork() 推演，JEPA 预测误差 | 零 LLM call |
| cost_remaining | 计数器 | 零 LLM call |

### TTS 渐进式预算控制

| 级别 | cost_remaining | 行为 |
|---|---|---|
| Normal | > 0.5 | 正常推理，ToT beam search 允许 |
| Degraded | 0.2 ~ 0.5 | 禁用 ToT，单步推理 |
| Urgent | 0.05 ~ 0.2 | 只走 Reaction + 直接回答，禁止新工具调用 |
| Abort | < 0.05 | 预算耗尽，终止 |

## 特性

- **三信号融合** — verifier + pred_error + cost_remaining，三路独立信号打破单 Verifier 自我闭环
- **TTS 分级控制** — 不是只有耗尽才停，预算消耗到阈值时主动降级推理策略
- **概念漂移检测** — `AnomalyDetector` 滑动窗口检测校准分数退化趋势
- **InteractionPattern 消费** — 消费 `MidTermWS.recent_pattern`，检测循环模式 → 自动切换策略
- **在线校准** — `calibrate_with_outcome()` 根据实际结果 EMA 更新预测误差 + 追加校准记录
- **可配置权重** — `MetaScoreWeights` 三路权重可调
- **环形历史缓冲** — `CalibrationHistory` 保留最近 1000 条记录

## 安装

```toml
[dependencies]
agent-metacognition = { path = "../agent-metacognition" }
```

## 快速上手

### 基础用法

```rust
use agent_metacognition::{
    Metacognition, MetaAction, MetaScoreWeights,
    calibrate::CalibrationModel,
};
use agent_state::AgentState;
use chrono::Duration;

// 1. 实现 CalibrationModel（本地小模型或 mock）
struct MyCalibrator;
#[async_trait::async_trait]
impl CalibrationModel for MyCalibrator {
    async fn calibrate(&self, state: &AgentState, decision_text: &str) -> CalibrationResult {
        CalibrationResult {
            raw_confidence: 0.85,
            calibrated_confidence: 0.85,
            should_retry: false,
            reasoning: "decision looks reasonable".into(),
        }
    }
}

// 2. 创建 Metacognition 实例
let metacog = Metacognition::new(
    Box::new(MyCalibrator),
    10_000,                    // token_budget
    Duration::seconds(120),    // time_budget
    5,                         // retry_budget
);

// 3. 每步决策后评估
let state = AgentState::new();
let assessment = metacog.evaluate(&state, "click the submit button").await;

match assessment.suggested_action {
    MetaAction::Proceed => { /* 正常执行 */ }
    MetaAction::RetryDecision => { /* 回滚重试 */ }
    MetaAction::SwitchStrategy => { /* 切换推理模式 */ }
    _ => {}
}

println!("meta_score: {:.2}", assessment.meta_score);
```

### 在线校准

```rust
let mut metacog = Metacognition::new(
    Box::new(MyCalibrator),
    10_000,
    Duration::seconds(120),
    5,
);

let mut state = AgentState::new();
let actual = state.clone();

// 根据执行结果校准
metacog.calibrate_with_outcome(
    &mut state,
    &actual,
    &assessment.calibration,
    assessment.meta_score,
);

// state.long_term.accumulated_pred_error 已更新
// CalibrationHistory 已追加记录
// AnomalyDetector 已更新
```

### TTS 信号

```rust
use agent_state::long::BudgetConsumed;

let mut consumed = BudgetConsumed::new();
consumed.tokens_used = 6_000; // 40% remaining

let signal = metacog.tts_signal(&consumed);
// → TTSSignal::Degraded { disable_tot: true }
```

### 异常检测

```rust
// AnomalyDetector 自动在 calibrate_with_outcome 中更新
// 也可手动使用：
use agent_metacognition::anomaly::AnomalyDetector;

let mut detector = AnomalyDetector::default(); // window=50, threshold=0.2
detector.update(metacog.calibration_history());

if detector.detect_drift() {
    // 概念漂移 → MetaAction::SwitchStrategy
}
```

### 自定义权重

```rust
use agent_metacognition::evaluate::MetaScoreWeights;

let metacog = Metacognition::new(
    Box::new(MyCalibrator),
    10_000,
    Duration::seconds(120),
    5,
).with_weights(MetaScoreWeights {
    verifier: 0.6,
    pred_error: 0.2,
    cost_remaining: 0.2,
});
```

## 核心类型

### Metacognition

```rust
pub struct Metacognition {
    calibration_model: Box<dyn CalibrationModel>,
    calibration_history: CalibrationHistory,
    anomaly_detector: AnomalyDetector,
    weights: MetaScoreWeights,
    token_budget: u64,
    time_budget: Duration,
    retry_budget: u32,
}
```

方法：

| 方法 | 说明 |
|---|---|
| `new(model, token, time, retry)` | 创建实例 |
| `with_weights(weights)` | 设置三信号权重 |
| `evaluate(state, decision_text)` | 三信号融合 → MetacognitiveAssessment |
| `calibrate_with_outcome(state, actual, cal, meta)` | 在线校准：更新 pred_error + 追加记录 + 更新异常检测 |
| `compute_cost_remaining(consumed)` | 剩余预算比例 |
| `tts_signal(consumed)` | TTS 分级信号 |
| `calibration_history()` | 访问校准历史 |
| `weights()` | 访问权重配置 |

### MetacognitiveAssessment

```rust
pub struct MetacognitiveAssessment {
    pub calibration: CalibrationResult,
    pub meta_score: f32,          // [0, 1]
    pub knows_unknown: bool,      // meta < 0.4
    pub concept_drifting: bool,   // 概念漂移
    pub budget_exhausted: bool,   // cost ≤ 0.05
    pub suggested_action: MetaAction,
}
```

### MetaAction 枚举

```rust
pub enum MetaAction {
    Proceed,               // 正常放行
    RetryDecision,         // 回滚重试
    RequestClarification,  // 向用户提问
    SwitchStrategy,        // 切换推理模式
    DelegateToHuman,       // 升级给人类
    AbortOnBudget,         // 预算耗尽
}
```

### CalibrationModel trait

```rust
#[async_trait]
pub trait CalibrationModel: Send + Sync {
    async fn calibrate(
        &self,
        state: &AgentState,
        decision_text: &str,
    ) -> CalibrationResult;
}
```

### TTSSignal 枚举

```rust
pub enum TTSSignal {
    Normal,
    Degraded { disable_tot: bool },
    Urgent { allow_reaction: bool, allow_new_tool: bool },
    Abort,
}
```

## 决策逻辑

`evaluate()` 的 MetaAction 决策优先级：

```
1. budget_exhausted             → AbortOnBudget
2. pattern_loop ∥ low_success   → SwitchStrategy  (消费 InteractionPattern)
3. knows_unknown (meta < 0.4)   → RequestClarification
4. concept_drifting             → SwitchStrategy  (AnomalyDetector)
5. cal.should_retry             → RetryDecision
6. 以上均不满足                  → Proceed
```

## 目录结构

```
src/
├── lib.rs          // MetaAction 枚举 + 模块声明 + re-exports
├── evaluate.rs     // Metacognition + MetacognitiveAssessment + 三信号融合
├── calibrate.rs    // CalibrationModel trait + CalibrationResult
├── tts.rs          // TTSSignal + classify_tts() + compute_cost_remaining
├── anomaly.rs      // AnomalyDetector 滑动窗口漂移检测
└── history.rs      // CalibrationRecord + CalibrationHistory 环形缓冲
```

## 与其他维度的关系

```
agent-metacognition ── 读 ──▶ agent-state   (accumulated_pred_error, recent_pattern, budget_consumed)
                    ── 写 ──▶ agent-state   (calibrate_with_outcome → update_pred_error)
                    ── 返回 ──▶ agent-session (MetaAction 分支处理)
                    ── 对比 ──▶ Monitor/Sidecar (在线 vs 异步异常检测)
```

### 在线 Metacognition vs Sidecar Monitor

| | Metacognition（在线） | Monitor（Sidecar） |
|---|---|---|
| 触发 | 每步决策后 | 异步、节流（60s+ 异常） |
| 延迟要求 | < 200ms | 无要求 |
| 职责 | 单步校准："这一步对吗？" | 全局检测："最近在退化吗？" |
| 输出 | MetaAction | MetacognitiveReport |

## 测试

```bash
cargo test -p agent-metacognition
```

覆盖：三信号融合公式 + 各 MetaAction 决策（Proceed/Retry/SwitchStrategy/AbortOnBudget/RequestClarification）、TTS 四级分档边界、异常检测器漂移/稳定、calibrate_with_outcome 更新、CalibrationHistory 环形缓冲。

## 依赖

- `agent-state` — 读取 pred_error、recent_pattern、budget_consumed
- `agent-types-core` — 基础类型
- `async-trait` — async trait 支持
- `serde` + `chrono` — 序列化与时间戳
- `tokio` — async 运行时

## License

与仓库一致。
