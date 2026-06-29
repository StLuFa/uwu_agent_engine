//! DelegationPolicy + DiscoveryStrategy + FallbackStrategy

use serde::{Deserialize, Serialize};

/// Agent 发现策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryStrategy {
    /// 精确匹配能力
    ExactCapability,
    /// 负载均衡
    LoadBalanced,
    /// 信任度排序
    TrustRanked,
    /// 竞标
    Auction,
}

/// 失败后的回退策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackStrategy {
    /// 重试同一 Agent
    RetrySame,
    /// 换一个 Agent
    TryNext,
    /// 升级给人类
    EscalateToHuman,
    /// 取消任务
    Cancel,
}

/// 委派策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPolicy {
    pub discovery: DiscoveryStrategy,
    pub fallback: FallbackStrategy,
    pub max_retries: u32,
    pub timeout_secs: u64,
}

impl Default for DelegationPolicy {
    fn default() -> Self {
        Self {
            discovery: DiscoveryStrategy::ExactCapability,
            fallback: FallbackStrategy::TryNext,
            max_retries: 3,
            timeout_secs: 300,
        }
    }
}
