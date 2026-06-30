# uwu_logger

基础日志抽象。当前为占位实现（println），生产环境建议替换为 `tracing`。

## 类型

| 类型 | 说明 |
|---|---|
| `Logger` | 日志门面 |
| `LogLevel` | 日志级别（Info / Warn / Error / Debug） |

## 状态

当前全仓直接使用 `println!/eprintln!` 进行日志输出，此 crate 尚未被任何模块消费。生产化时建议：

- 替换为 `tracing` + `tracing-subscriber`
- 或在此 crate 中实现 `tracing` 的 Layer

## 消费者

暂无。
