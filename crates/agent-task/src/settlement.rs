//! SettlementPolicy + SettlementMode

use serde::{Deserialize, Serialize};

/// 结算模式
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SettlementMode {
    /// 免费协作
    Free,
    /// 固定价格
    FixedPrice { amount: u64 },
    /// 按用量计费
    Metered { price_per_token: f64 },
    /// 竞标
    Auction { reserve_price: u64 },
}

/// 结算策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementPolicy {
    pub mode: SettlementMode,
    pub payer: String,
    pub payee: String,
}

impl Default for SettlementPolicy {
    fn default() -> Self {
        Self {
            mode: SettlementMode::Free,
            payer: String::new(),
            payee: String::new(),
        }
    }
}

impl SettlementPolicy {
    pub fn free() -> Self {
        Self::default()
    }

    pub fn fixed(amount: u64, payer: impl Into<String>, payee: impl Into<String>) -> Self {
        Self {
            mode: SettlementMode::FixedPrice { amount },
            payer: payer.into(),
            payee: payee.into(),
        }
    }
}
