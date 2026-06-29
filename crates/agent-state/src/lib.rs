//! # agent-state
//!
//! Agent 状态 —— 短/中/长程三层工作状态 + fork() 推演沙盒 + MVCC 快照。
//!
//! State 是 Agent 对"世界长什么样 + 任务进行到哪"的结构化理解，
//! 是 Agent 五维中最核心的维度——所有决策基于它，而非 scratchpad 文本。
//!
//! ## 三层 WS
//!
//! | 层级 | 更新频率 | 版本号 | 内容 |
//! |---|---|---|---|
//! | ShortTermWS | 每步 +1 | short_term.version | 当前上下文、上一步动作、暂存假设 |
//! | MidTermWS | 每 N 步 +1 | mid_term.version | 动作历史、已知事实、交互模式 |
//! | LongTermWS | 任务级 +1 | long_term.version | 任务进度、累积预测误差、预算消耗 |
//!
//! ## MVCC 并发
//!
//! - 主进程：读写 State，写入时增加对应层版本号
//! - Sidecar：通过 `snapshot()` 获取只读快照，不阻塞主进程
//! - 快照版本号 = max(short.version, mid.version, long.version)

pub mod checkpoint;
pub mod confidence;
pub mod diff;
pub mod evaluate;
pub mod long;
pub mod mid;
pub mod mvcc;
pub mod short;
pub mod state;

pub use checkpoint::{StateCheckpoint, rollback};
pub use confidence::ConfidenceMap;
pub use diff::StateDiff;
pub use evaluate::{StateScore, evaluate};
pub use long::LongTermWS;
pub use mid::{InteractionPattern, MidTermWS};
pub use mvcc::StateSnapshot;
pub use short::ShortTermWS;
pub use state::AgentState;
