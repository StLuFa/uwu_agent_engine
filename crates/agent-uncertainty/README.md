# agent-uncertainty

贝叶斯不确定性估计 — 用于 Agent 决策的置信度量化。

## 核心概念

- **Beta 分布** — 伯努利试验的共轭先验。先验 Beta(α, β) + 观察 s 次成功 + f 次失败 → 后验 Beta(α+s, β+f)
- **可信区间** — 95% HDI（Highest Posterior Density）
- **贝叶斯聚合** — 逆方差加权融合多个 belief

## 类型

| 类型 | 说明 |
|---|---|
| `BetaDistribution` | Beta(α, β) 分布：mean / variance / pdf / credible interval |
| `BetaBelief` | 包装 Beta 分布 + 观察计数 + 置信度估计 |
| `BeliefEstimate` | 单 belief 估计：mean / uncertainty / CI / 有效样本数 |
| `BayesianAggregator` | 逆方差加权聚合多个 belief → 整体不确定性 |
| `UncertaintyEstimate` | 聚合结果：overall + per-dimension + should_confirm |
| `CredibleInterval` | [lower, upper] 区间的宽度和包含性检查 |

## 使用

```rust
use agent_uncertainty::{BetaBelief, BetaDistribution, BayesianAggregator};

// 追踪决策质量
let mut belief = BetaBelief::uniform("tool_reliability");
belief.observe_success();
belief.observe_success();
belief.observe_failure();  // Beta(3, 2)，均值 0.6

let est = belief.estimate();
// est.mean ≈ 0.6, est.uncertainty < 1.0

// 聚合多个 belief
let agg = BayesianAggregator::new(0.7);
let overall = agg.aggregate(&[belief]);
// overall.should_confirm 当 uncertainty > 0.7 时为 true
```

## 数学

- **均值**: E[X] = α / (α + β)
- **方差**: Var[X] = αβ / ((α+β)²(α+β+1))
- **95% CI**: 正态近似 mean ± 1.96·σ (α,β > 5 时 < 1% 误差)
- **逆方差权重**: wᵢ = 1 / max(varianceᵢ, 1e-6)

## 测试

```bash
cargo test -p agent-uncertainty  # 16 passed
```

## 消费者

- `agent-metacognition` — BayesianCalibrator 追踪决策质量
