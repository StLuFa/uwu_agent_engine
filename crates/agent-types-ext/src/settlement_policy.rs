//! SettlementPolicy / SettlementMode

use serde::{Deserialize, Serialize};

/// 结算模式
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SettlementMode {
    Free,
    FixedPrice { amount: u64 },
    Metered { price_per_token: f64 },
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
