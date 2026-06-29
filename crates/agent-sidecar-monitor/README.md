# agent-sidecar-monitor

**独立监控进程** —— 异常检测引擎 + MetacognitiveReport 定期生成。

## 概述

Monitor 作为独立进程运行，消费 monitoring 通道的 pred_error 指标，使用滑动窗口检测概念漂移，定期生成 MetacognitiveReport。

```
Monitoring 通道
  │
  ├─ 1. 接收 pred_error 事件
  ├─ 2. AnomalyDetector.feed(pred_error)
  ├─ 3. 定期 update_baseline() (EMA)
  ├─ 4. 检测漂移 (baseline - current > threshold)
  └─ 5. generate_report() → MetacognitiveReport
```

## 运行

```bash
cargo run -p agent-sidecar-monitor
```

输出：
```
[monitor] starting...
[monitor] report: stable: baseline=0.502, current=0.201 (drift=false)
[monitor] report: DRIFT DETECTED: baseline=0.514, current=0.598 (drift=true)
[monitor] processed 100 events, shutting down
```

## 核心类型

### AnomalyDetector

| 参数 | 默认值 | 说明 |
|---|---|---|
| window_size | 50 | 滑动窗口大小 |
| drift_threshold | 0.2 | 漂移判定阈值 |
| baseline | 0.5 | EMA 基线 |

方法：`feed(value)`, `current_mean()`, `update_baseline()`, `is_drifting()`, `anomaly_count()`, `generate_report()`

### MetacognitiveReport

```rust
struct MetacognitiveReport {
    report_id: String,
    generated_at: DateTime<Utc>,
    window_secs: u64,
    total_events: u64,
    anomaly_count: u64,
    drift_detected: bool,
    avg_pred_error: f32,
    summary: String,
}
```

## 流程

| 步骤 | 说明 |
|---|---|
| 初始化 | AnomalyDetector(window=50, threshold=0.2) |
| 主循环 | mock pred_error → feed → 每 10s 生成 report |
| baseline 更新 | EMA: `0.9 * old + 0.1 * current_mean` |
| 漂移检测 | `|baseline - current_mean| > threshold` |
| 异常计数 | 窗口内超过 baseline+threshold 的事件数 |

## 后续集成

- 接 agent-mesh → 消费真实 monitoring 通道
- 接 Metacognition → 读取 meta_score 流
- 告警输出 → OpenTelemetry / Webhook / Slack

## 依赖

- `agent-state` — AgentState 类型
- `serde` + `chrono` + `uuid` — 序列化与标识
- `tokio` — async 运行时

## License

与仓库一致。
