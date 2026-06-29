# uwu_agent_engine

**反应式 AI Agent 引擎** —— Agent 不是管道，是人。

一个完整的 Agent 由五个正交维度构成。市面框架把它们塞进 prompt 或 scratchpad 里；uwu_agent_engine 把它们拆成独立一等概念，各自独立 crate、独立生命周期、独立可测试。

## 五维架构

| 维度 | Crate | 核心能力 |
|---|---|---|
| **Reaction** 反应 | `agent-reaction` | 规则触发短路，命中则跳过 LLM，省 30-50% token |
| **State** 状态 | `agent-state` | 短/中/长程三层 + fork()推演沙盒 + MVCC 并发 + JEPA 预测误差 |
| **Metacognition** 元认知 | `agent-metacognition` | 三信号在线自校准 + TTS 渐进式预算控制 |
| **Persona** 人物角色 | `agent-persona` | 身份/关系网络/履历（可变，MVCC 版本化） |
| **Character** 人格 | `agent-character` | 核心价值观（不可变）+ 决策偏好（可调） |

## 决策主循环

```
每步请求:
  1. Reaction.intercept(state)  ── Hit ──▶  0 token 直接执行
  2. FlowGraph: P→M→R→E        ──▶  生成 Action
  3. Metacognition.evaluate()   ──▶  MetaAction 建议
  4. MetaAction 分支处理         ──▶  Proceed/Retry/Clarify/Switch/Abort
  5. Execution + GuardLayer     ──▶  五层闸门检查 → 执行
  6. Metacognition.calibrate()  ──▶  EMA 更新预测误差
```

## 能力域 (P→M→R→E)

| 阶段 | Crate | 职责 |
|---|---|---|
| **Perception** | `agent-perception` | 输入解析 + PII 检测与处理 |
| **Memory** | `agent-memory` | 统一记忆（四型视图 + 向量检索 + Episode 巩固） |
| **Reasoning** | `agent-reasoning` | fork() 沙盒推演 + ToT beam search |
| **Execution** | `agent-execution` | MCP 工具调用 + 输出格式化 |
| **Orchestration** | `agent-core` | FlowGraph 管道 + FlowEngine + CapabilityRegistry |

## 安全与协作

| Crate | 职责 |
|---|---|
| `agent-guard` | 五层硬闸门（指令/参数/能力/预算/egress），编译期注册不可绕过 |
| `agent-learning` | LearnNode 条件触发 + Skill 版本化 + 5 层自进化防护 |
| `agent-task` | TaskManifest + SubtaskDAG 调度 + SettlementPolicy |
| `agent-collaboration` | 多 Agent 委派/协商 + AgentRegistry 能力索引 |
| `agent-session` | 持有五维，完整 process_turn 6 段式主循环 |
| `agent-mesh` | Agent 语义事件网格（9 种事件类型 + TypeRegistry） |
| `agent-wiki` | 多 Agent 协作知识库（MVCC 版本化 + 可插拔存储） |

## 基础设施

| Crate | 职责 |
|---|---|
| `uwu_event_mesh` | 进程内事件网格（层级 topic pub/sub + 四路通道 + 持久化回放） |
| `uwu_visual_script` | 可视化脚本引擎（Graph→ExecutionPlan→SlotProgram→VM） |
| `uwu_wasm` | WASM 沙箱引擎（Component Model + WASI Preview 2 + 时间旅行调试） |
| `uwu_database` | 统一数据访问层（SQL + 缓存 + 向量存储） |
| `uwu_logger` | 日志系统 |

## Sidecar 独立进程

| 进程 | 职责 |
|---|---|
| `consolidator` | 消费 Episode → LearnTrigger 评估 → Guard 博弈 → Memory 持久化 |
| `monitor` | 滑动窗口异常检测 + MetacognitiveReport 定期生成 |

## 快速开始

```bash
# 构建所有 crate
cargo build --workspace

# 运行测试
cargo test -p agent-state
cargo test -p agent-reaction
# ... 每个 crate 独立可测试

# 运行 Sidecar
cargo run -p agent-sidecar-consolidator
cargo run -p agent-sidecar-monitor
```

## 测试覆盖

19 个已实现 crate，累计 **195 个测试，0 失败**：

| Crate | Tests |
|---|---|
| agent-state | 21 |
| agent-reaction | 22 |
| agent-metacognition | 16 |
| agent-persona | 5 |
| agent-character | 6 |
| agent-mesh | 11 |
| agent-perception | 15 |
| agent-memory | 10 |
| agent-reasoning | 12 |
| agent-execution | 9 |
| agent-core | 13 |
| agent-session | 11 |
| agent-task | 8 |
| agent-collaboration | 5 |
| agent-learning | 7 |
| agent-guard | 12 |
| agent-wiki | 12 |

## 目录结构

```
uwu_agent_engine/
├── Cargo.toml              # Workspace (27 crates)
├── ARCHITECTURE.md          # 完整系统架构文档
├── ROADMAP.md               # 实施路线图（阶段 0-8 已完成）
├── README.md                # 本文件
├── pkg/                     # 5 个基础设施 crate
│   ├── uwu_event_mesh/
│   ├── uwu_visual_script/
│   ├── uwu_wasm/
│   ├── uwu_database/
│   └── uwu_logger/
└── crates/                  # 22 个 Agent 领域 crate
    ├── agent-types-core/    # 基础类型（冻结）
    ├── agent-types-ext/     # 业务类型
    ├── agent-state/         # ★ 状态维度
    ├── agent-reaction/      # ★ 反射维度
    ├── agent-metacognition/ # ★ 元认知维度
    ├── agent-persona/       # ★ 人物角色维度
    ├── agent-character/     # ★ 人格维度
    ├── agent-mesh/          # 事件网格
    ├── agent-perception/    # 感知域
    ├── agent-memory/        # 统一记忆
    ├── agent-reasoning/     # 推理域
    ├── agent-execution/     # 执行域
    ├── agent-core/          # FlowGraph + FlowEngine
    ├── agent-session/       # 对话域
    ├── agent-task/          # 任务域
    ├── agent-collaboration/ # 多 Agent 协作
    ├── agent-learning/      # 自学习
    ├── agent-guard/         # 安全守卫
    ├── agent-wiki/          # 协作知识库
    ├── agent-sidecar-consolidator/  # 巩固进程
    └── agent-sidecar-monitor/       # 监控进程
```

## 技术栈

- **语言**: Rust (edition 2024)
- **异步**: Tokio
- **序列化**: Serde (JSON)
- **事件网格**: 自研 uwu_event_mesh（层级 topic pub/sub + 四路通道）
- **图执行**: 自研 uwu_visual_script（Graph → SlotProgram → VM）
- **WASM**: wasmtime 37（Component Model + WASI Preview 2）
- **数据库**: SQLx (PostgreSQL/MySQL/SQLite) + Qdrant/Pgvector/LanceDB

## 设计原则

- **State 唯一真相源**：所有决策基于 AgentState，不是 scratchpad 文本
- **Reaction 优先**：高频低智操作短路跳过 LLM
- **事件即契约**：SerializedEnvelope + TypeRegistry 跨进程类型安全
- **MVCC 并发**：主进程写，Sidecar 读快照，零阻塞
- **GuardLayer 不可绕过**：编译期注册，运行时不可自提升
- **能力动态加载**：CapabilityRegistry trait object 插件式扩展
- **编排显式化**：FlowGraph 声明式管道，支持热更新

## 文档

- [ARCHITECTURE.md](ARCHITECTURE.md) — 完整系统架构设计
- [ROADMAP.md](ROADMAP.md) — 实施路线图与进度
- 每个 crate 均有独立的 README.md

## License

MIT
