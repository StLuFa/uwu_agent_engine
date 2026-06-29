//! DelegationPolicy / DiscoveryStrategy / FallbackStrategy

use serde::{Deserialize, Serialize};

/// Agent 发现策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryStrategy {
    ExactCapability,
    LoadBalanced,
    TrustRanked,
    Auction,
}

/// 失败回退策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackStrategy {
    RetrySame,
    TryNext,
    EscalateToHuman,
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
