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

    /// 计算费用
    pub fn calculate_cost(&self, tokens_used: u64) -> u64 {
        match self.mode {
            SettlementMode::Free => 0,
            SettlementMode::FixedPrice { amount } => amount,
            SettlementMode::Metered { price_per_token } => {
                (tokens_used as f64 * price_per_token) as u64
            }
            SettlementMode::Auction { reserve_price } => reserve_price,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_settlement_zero_cost() {
        let policy = SettlementPolicy::free();
        assert_eq!(policy.calculate_cost(1000), 0);
    }

    #[test]
    fn fixed_price_ignores_tokens() {
        let policy = SettlementPolicy::fixed(500, "payer", "payee");
        assert_eq!(policy.calculate_cost(0), 500);
        assert_eq!(policy.calculate_cost(9999), 500);
    }

    #[test]
    fn metered_scales_with_tokens() {
        let policy = SettlementPolicy {
            mode: SettlementMode::Metered {
                price_per_token: 0.001,
            },
            payer: "a".into(),
            payee: "b".into(),
        };
        assert_eq!(policy.calculate_cost(5000), 5); // 5000 * 0.001 = 5
    }

    #[test]
    fn auction_uses_reserve_price() {
        let policy = SettlementPolicy {
            mode: SettlementMode::Auction { reserve_price: 1000 },
            payer: "a".into(),
            payee: "b".into(),
        };
        assert_eq!(policy.calculate_cost(0), 1000);
    }
}
