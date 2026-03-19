//! Content filter for research suggestions.
//!
//! Trait-based so we can swap implementations:
//! - `NoopFilter`: always accepts (demo)
//! - Future: BART classifier → cheap LLM two-layer pipeline

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum FilterResult {
    Accept,
    Reject { reason: String },
}

#[async_trait]
pub trait ContentFilter: Send + Sync {
    async fn check(&self, text: &str) -> FilterResult;
}

/// Demo filter: accepts everything.
pub struct NoopFilter;

#[async_trait]
impl ContentFilter for NoopFilter {
    async fn check(&self, _text: &str) -> FilterResult {
        FilterResult::Accept
    }
}
