//! Rate limiting middleware for unauthenticated auth endpoints.
//!
//! Builds [`GovernorLayer`] instances keyed by client IP. When `enabled:
//! false` the helper returns `None` so callers skip the layer entirely,
//! keeping tests and local dev free of rate-limit interference.
//!
//! Key extractor: [`FallbackIpKeyExtractor`] — prefers `X-Forwarded-For` /
//! `X-Real-IP` / `Forwarded` headers then falls back to peer IP. If no IP
//! can be extracted at all (e.g. unix socket), it falls back to `0.0.0.0`
//! rather than failing the request (fail-open for availability).
//!
//! Error response: JSON `{"error": "..."}` with `Retry-After`, produced by
//! the shared [`crate::http::too_many_requests`] helper.

use std::net::{IpAddr, Ipv4Addr};

use axum::{body::Body, http::HeaderValue, response::IntoResponse};
use governor::middleware::NoOpMiddleware;
use tower_governor::{
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::{KeyExtractor, SmartIpKeyExtractor},
    GovernorError, GovernorLayer,
};

use crate::config::RateLimitConfig;
use crate::http::too_many_requests;

/// Fallback IP `0.0.0.0` used when the peer address cannot be extracted.
const FALLBACK_IP: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);

/// Key extractor that wraps [`SmartIpKeyExtractor`].
///
/// Falls back to `0.0.0.0` when no IP can be determined rather than returning
/// an error, ensuring rate limiting never blocks requests due to key extraction
/// failure (fail-open for availability).
#[derive(Clone)]
pub struct FallbackIpKeyExtractor;

impl KeyExtractor for FallbackIpKeyExtractor {
    type Key = IpAddr;

    fn extract<B>(&self, req: &axum::http::Request<B>) -> Result<Self::Key, GovernorError> {
        Ok(SmartIpKeyExtractor.extract(req).unwrap_or(FALLBACK_IP))
    }
}

/// Concrete governor config type used throughout this module.
pub type IpGovernorConfig = GovernorConfig<FallbackIpKeyExtractor, NoOpMiddleware>;

/// Concrete governor layer type returned by this module.
pub type IpGovernorLayer = GovernorLayer<FallbackIpKeyExtractor, NoOpMiddleware, Body>;

/// Build a [`GovernorLayer`] that allows `per_minute` requests per IP per minute.
///
/// The burst size equals `per_minute` — a client can use the full minute's
/// allowance up-front, then tokens replenish at one per `(60 / per_minute)`
/// seconds.
///
/// Returns `None` when `config.enabled` is `false` or `per_minute` is zero.
#[must_use]
pub fn make_governor_layer(per_minute: u32, config: &RateLimitConfig) -> Option<IpGovernorLayer> {
    if !config.enabled || per_minute == 0 {
        return None;
    }

    // One token replenished every (60 / per_minute) seconds.
    // For 5/min → 12 s/token; for 10/min → 6 s/token.
    let secs_per_token = (60u64).checked_div(u64::from(per_minute)).unwrap_or(1);
    let secs_per_token = secs_per_token.max(1); // floor at 1 s to satisfy governor

    // key_extractor() takes &mut self and returns a new GovernorConfigBuilder<K2, M>.
    // Bind the default builder first so we can take &mut of it.
    let mut base = GovernorConfigBuilder::default();
    let mut builder = base.key_extractor(FallbackIpKeyExtractor);
    builder.per_second(secs_per_token).burst_size(per_minute);
    let gov_config: IpGovernorConfig = builder.finish()?;

    let layer = GovernorLayer::new(gov_config).error_handler(|e: GovernorError| {
        let wait_secs = match &e {
            GovernorError::TooManyRequests { wait_time, .. } => *wait_time,
            _ => 60,
        };
        let mut resp = too_many_requests("Too many requests — please slow down").into_response();
        if let Ok(val) = HeaderValue::from_str(&wait_secs.to_string()) {
            resp.headers_mut()
                .insert(axum::http::header::RETRY_AFTER, val);
        }
        resp
    });

    Some(layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> RateLimitConfig {
        RateLimitConfig {
            enabled: true,
            signup_per_minute: 5,
            login_per_minute: 10,
            backup_per_minute: 10,
        }
    }

    #[test]
    fn disabled_config_returns_none() {
        let config = RateLimitConfig {
            enabled: false,
            ..enabled_config()
        };
        assert!(make_governor_layer(5, &config).is_none());
    }

    #[test]
    fn zero_per_minute_returns_none() {
        let config = enabled_config();
        assert!(make_governor_layer(0, &config).is_none());
    }

    #[test]
    fn enabled_config_returns_some() {
        let config = enabled_config();
        assert!(make_governor_layer(5, &config).is_some());
        assert!(make_governor_layer(10, &config).is_some());
    }
}
