//! Security headers middleware for HTTP responses.
//!
//! This module provides middleware for adding security headers to HTTP responses,
//! including CSP, HSTS, X-Frame-Options, and other protective headers.

use std::sync::Arc;

use axum::{
    extract::Request,
    http::{
        header::{
            CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY,
            X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION,
        },
        HeaderMap, HeaderValue,
    },
    middleware::Next,
    response::Response,
    Extension,
};

use crate::config::SecurityHeadersConfig;

/// Build security headers from configuration.
///
/// Returns an `Arc`-wrapped `HeaderMap` that can be shared across requests
/// via Axum's `Extension` layer.
#[must_use]
pub fn build_security_headers(config: &SecurityHeadersConfig) -> Arc<HeaderMap> {
    let mut headers = HeaderMap::new();

    // X-Content-Type-Options: nosniff (always)
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));

    // X-Frame-Options
    if let Ok(value) = HeaderValue::from_str(&config.frame_options) {
        headers.insert(X_FRAME_OPTIONS, value);
    }

    // X-XSS-Protection (legacy but still useful for older browsers)
    headers.insert(X_XSS_PROTECTION, HeaderValue::from_static("1; mode=block"));

    // Content-Security-Policy
    if let Ok(value) = HeaderValue::from_str(&config.content_security_policy) {
        headers.insert(CONTENT_SECURITY_POLICY, value);
    }

    // Referrer-Policy
    if let Ok(value) = HeaderValue::from_str(&config.referrer_policy) {
        headers.insert(REFERRER_POLICY, value);
    }

    // HSTS (only if enabled - should only be used with HTTPS)
    if config.hsts_enabled {
        let hsts_value = if config.hsts_include_subdomains {
            format!("max-age={}; includeSubDomains", config.hsts_max_age)
        } else {
            format!("max-age={}", config.hsts_max_age)
        };
        if let Ok(value) = HeaderValue::from_str(&hsts_value) {
            headers.insert(STRICT_TRANSPORT_SECURITY, value);
        }
    }

    Arc::new(headers)
}

/// Middleware to add security headers to all responses.
///
/// This middleware reads the pre-built `HeaderMap` from an `Extension` and
/// extends every response with those headers. It should be added as the
/// outermost layer so headers are applied to all routes.
///
/// # Example
///
/// ```ignore
/// use axum::{middleware, Router, Extension};
/// use tinycongress_api::http::security::{build_security_headers, security_headers_middleware};
/// use tinycongress_api::config::SecurityHeadersConfig;
///
/// let config = SecurityHeadersConfig::default();
/// let headers = build_security_headers(&config);
///
/// let app = Router::new()
///     // ... routes ...
///     .layer(middleware::from_fn(security_headers_middleware))
///     .layer(Extension(headers));
/// ```
pub async fn security_headers_middleware(
    Extension(headers): Extension<Arc<HeaderMap>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let response_headers = response.headers_mut();
    for (k, v) in headers.iter() {
        response_headers.insert(k.clone(), v.clone());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_security_headers_default() {
        let config = SecurityHeadersConfig::default();
        let headers = build_security_headers(&config);

        // Should have at least the mandatory headers
        assert!(headers.contains_key(X_CONTENT_TYPE_OPTIONS));
        assert!(headers.contains_key(X_FRAME_OPTIONS));
        assert!(headers.contains_key(X_XSS_PROTECTION));
        assert!(headers.contains_key(CONTENT_SECURITY_POLICY));
        assert!(headers.contains_key(REFERRER_POLICY));
    }

    #[test]
    fn test_build_security_headers_with_hsts() {
        let mut config = SecurityHeadersConfig::default();
        config.hsts_enabled = true;
        config.hsts_max_age = 31_536_000;
        config.hsts_include_subdomains = true;

        let headers = build_security_headers(&config);

        let hsts = headers
            .get(STRICT_TRANSPORT_SECURITY)
            .map(|v| v.to_str().unwrap_or_default());

        assert!(hsts.is_some());
        assert!(hsts.unwrap().contains("max-age=31536000"));
        assert!(hsts.unwrap().contains("includeSubDomains"));
    }

    #[test]
    fn test_build_security_headers_custom_frame_options() {
        let mut config = SecurityHeadersConfig::default();
        config.frame_options = "SAMEORIGIN".to_string();

        let headers = build_security_headers(&config);

        let frame_options = headers
            .get(X_FRAME_OPTIONS)
            .map(|v| v.to_str().unwrap_or_default());

        assert_eq!(frame_options, Some("SAMEORIGIN"));
    }
}
