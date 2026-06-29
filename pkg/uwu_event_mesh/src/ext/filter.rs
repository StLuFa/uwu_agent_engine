//! Declarative envelope filters for server-side payload pruning.
//!
//! A [`Filter`] is evaluated by the broker during fan-out: events that
//! don't match are not enqueued into the subscriber's channel, so a slow
//! consumer never wastes buffer slots on irrelevant traffic.
//!
//! ```ignore
//! use uwu_event_mesh::prelude::*;
//!
//! // Header equality (fast path, no JSON parsing).
//! let f = Filter::header("tenant", "acme");
//!
//! // JSON pointer existence + equality on payload.
//! let f = Filter::pointer_eq("/order/status", serde_json::json!("paid"));
//!
//! // Custom closure: full programmatic access.
//! let f = Filter::predicate(|env| env.payload["amount"].as_f64().unwrap_or(0.0) > 100.0);
//!
//! // Compose.
//! let f = Filter::all_of([f, Filter::header("region", "us")]);
//! ```

use std::sync::Arc;

use crate::core::envelope::Envelope;

pub type EnvelopePredicate = Arc<dyn Fn(&Envelope) -> bool + Send + Sync + 'static>;

/// Server-side envelope filter applied during fan-out.
#[derive(Clone)]
pub enum Filter {
    /// User-supplied closure. Must be cheap and side-effect free.
    Predicate(EnvelopePredicate),
    /// `headers[key] == value`.
    HeaderEq { key: String, value: String },
    /// `headers` contains `key` (any value).
    HeaderExists(String),
    /// `payload.pointer(p) == value` (uses RFC 6901 JSON Pointer).
    PointerEq {
        pointer: String,
        value: serde_json::Value,
    },
    /// `payload.pointer(p)` resolves.
    PointerExists(String),
    /// All children must match.
    And(Vec<Filter>),
    /// At least one child must match.
    Or(Vec<Filter>),
    /// Negation.
    Not(Box<Filter>),
}

impl std::fmt::Debug for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Filter::Predicate(_) => f.write_str("Predicate(<fn>)"),
            Filter::HeaderEq { key, value } => write!(f, "HeaderEq({key:?}, {value:?})"),
            Filter::HeaderExists(k) => write!(f, "HeaderExists({k:?})"),
            Filter::PointerEq { pointer, value } => write!(f, "PointerEq({pointer:?}, {value})"),
            Filter::PointerExists(p) => write!(f, "PointerExists({p:?})"),
            Filter::And(xs) => f.debug_tuple("And").field(xs).finish(),
            Filter::Or(xs) => f.debug_tuple("Or").field(xs).finish(),
            Filter::Not(x) => f.debug_tuple("Not").field(x).finish(),
        }
    }
}

impl Filter {
    pub fn predicate<F>(f: F) -> Self
    where
        F: Fn(&Envelope) -> bool + Send + Sync + 'static,
    {
        Self::Predicate(Arc::new(f))
    }

    pub fn header(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self::HeaderEq { key: key.into(), value: value.into() }
    }

    pub fn header_exists(key: impl Into<String>) -> Self {
        Self::HeaderExists(key.into())
    }

    pub fn pointer_eq(pointer: impl Into<String>, value: serde_json::Value) -> Self {
        Self::PointerEq { pointer: pointer.into(), value }
    }

    pub fn pointer_exists(pointer: impl Into<String>) -> Self {
        Self::PointerExists(pointer.into())
    }

    pub fn all_of<I: IntoIterator<Item = Filter>>(it: I) -> Self {
        Self::And(it.into_iter().collect())
    }

    pub fn any_of<I: IntoIterator<Item = Filter>>(it: I) -> Self {
        Self::Or(it.into_iter().collect())
    }

    pub fn not(self) -> Self {
        Self::Not(Box::new(self))
    }

    /// Evaluate the filter against an envelope.
    pub fn matches(&self, env: &Envelope) -> bool {
        match self {
            Filter::Predicate(f) => f(env),
            Filter::HeaderEq { key, value } => env.headers.get(key).map(|v| v == value).unwrap_or(false),
            Filter::HeaderExists(k) => env.headers.contains_key(k),
            Filter::PointerEq { pointer, value } => {
                env.payload.pointer(pointer).map(|v| v == value).unwrap_or(false)
            }
            Filter::PointerExists(p) => env.payload.pointer(p).is_some(),
            Filter::And(xs) => xs.iter().all(|f| f.matches(env)),
            Filter::Or(xs) => xs.iter().any(|f| f.matches(env)),
            Filter::Not(x) => !x.matches(env),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::topic::Topic;
    use serde_json::json;

    fn env_with(headers: &[(&str, &str)], payload: serde_json::Value) -> Envelope {
        let t = Topic::new("a.b").unwrap();
        let mut e = Envelope::new(&t, payload);
        for (k, v) in headers {
            e.headers.insert((*k).to_string(), (*v).to_string());
        }
        e
    }

    #[test]
    fn header_eq() {
        let e = env_with(&[("region", "us")], json!({}));
        assert!(Filter::header("region", "us").matches(&e));
        assert!(!Filter::header("region", "eu").matches(&e));
    }

    #[test]
    fn pointer_eq_and_exists() {
        let e = env_with(&[], json!({"order": {"status": "paid", "amount": 42}}));
        assert!(Filter::pointer_eq("/order/status", json!("paid")).matches(&e));
        assert!(!Filter::pointer_eq("/order/status", json!("pending")).matches(&e));
        assert!(Filter::pointer_exists("/order/amount").matches(&e));
        assert!(!Filter::pointer_exists("/order/missing").matches(&e));
    }

    #[test]
    fn boolean_combinators() {
        let e = env_with(&[("tenant", "acme")], json!({"v": 5}));
        let f = Filter::all_of([
            Filter::header("tenant", "acme"),
            Filter::pointer_exists("/v"),
        ]);
        assert!(f.matches(&e));

        let g = Filter::any_of([
            Filter::header("tenant", "other"),
            Filter::pointer_eq("/v", json!(5)),
        ]);
        assert!(g.matches(&e));

        let n = Filter::header("tenant", "other").not();
        assert!(n.matches(&e));
    }

    #[test]
    fn predicate_closure() {
        let e = env_with(&[], json!({"amount": 250}));
        let f = Filter::predicate(|env| env.payload["amount"].as_i64().unwrap_or(0) > 100);
        assert!(f.matches(&e));
    }
}
