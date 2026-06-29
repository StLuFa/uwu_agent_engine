//! NegotiationResult

use serde::{Deserialize, Serialize};

/// 协商结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationResult {
    pub accepted: bool,
    pub reason: Option<String>,
    pub agreed_price: Option<u64>,
    pub counter_offer: Option<u64>,
}

impl NegotiationResult {
    pub fn accepted(price: Option<u64>) -> Self {
        Self {
            accepted: true,
            reason: None,
            agreed_price: price,
            counter_offer: None,
        }
    }

    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason: Some(reason.into()),
            agreed_price: None,
            counter_offer: None,
        }
    }

    pub fn counter_offer(price: u64) -> Self {
        Self {
            accepted: false,
            reason: Some("counter offer".into()),
            agreed_price: None,
            counter_offer: Some(price),
        }
    }
}
