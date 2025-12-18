//! Security headers middleware for HTTP responses.
//!
//! This module provides middleware for adding security headers to HTTP responses,
//! including CSP, HSTS, X-Frame-Options, and other protective headers.

use std::sync::Arc;

use axum::{
    extract::Request,
    http::header::{
        HeaderName, HeaderValue, CONTENT_SECURITY_POLICY, REFERRER_POLICY,
        STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION,
    },
    middleware::Next,
    response::Response,
    Extension,
};

use crate::config::SecurityHeadersConfig;

/// Build security headers from configuration.
///
/// Returns an `Arc`-wrapped vector of header name/value pairs that can be
/// shared across requests via Axum's `Extension` layer.
#[must_use]
pub fn build_security_headers(
    config: &SecurityHeadersConfig,
) -> Arc<Vec<(HeaderName, HeaderValue)>> {
    let mut headers = Vec::new();

    // X-Content-Type-Options: nosniff (always)
    headers.push((X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")));

    // X-Frame-Options
    if let Ok(value) = HeaderValue::from_str(&config.frame_options) {
        headers.push((X_FRAME_OPTIONS, value));
    }

    // X-XSS-Protection (legacy but still useful for older browsers)
    headers.push((X_XSS_PROTECTION, HeaderValue::from_static("1; mode=block")));

    // Content-Security-Policy
    if let Ok(value) = HeaderValue::from_str(&config.content_security_policy) {
        headers.push((CONTENT_SECURITY_POLICY, value));
    }

    // Referrer-Policy
    if let Ok(value) = HeaderValue::from_str(&config.referrer_policy) {
        headers.push((REFERRER_POLICY, value));
    }

    // HSTS (only if enabled - should only be used with HTTPS)
    if config.hsts_enabled {
        let hsts_value = if config.hsts_include_subdomains {
            format!("max-age={}; includeSubDomains", config.hsts_max_age)
        } else {
            format!("max-age={}", config.hsts_max_age)
        };
        if let Ok(value) = HeaderValue::from_str(&hsts_value) {
            headers.push((STRICT_TRANSPORT_SECURITY, value));
        }
    }

    Arc::new(headers)
}

/// Middleware to add security headers to all responses.
///
/// This middleware reads the pre-built headers from an `Extension` and applies
/// them to every response. It should be added as the outermost layer so headers
/// are applied to all routes.
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
    Extension(headers): Extension<Arc<Vec<(HeaderName, HeaderValue)>>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let response_headers = response.headers_mut();
    for (name, value) in headers.iter() {
        response_headers.insert(name.clone(), value.clone());
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
        assert!(headers.iter().any(|(n, _)| n == X_CONTENT_TYPE_OPTIONS));
        assert!(headers.iter().any(|(n, _)| n == X_FRAME_OPTIONS));
        assert!(headers.iter().any(|(n, _)| n == X_XSS_PROTECTION));
        assert!(headers.iter().any(|(n, _)| n == CONTENT_SECURITY_POLICY));
        assert!(headers.iter().any(|(n, _)| n == REFERRER_POLICY));
    }

    #[test]
    fn test_build_security_headers_with_hsts() {
        let mut config = SecurityHeadersConfig::default();
        config.hsts_enabled = true;
        config.hsts_max_age = 31_536_000;
        config.hsts_include_subdomains = true;

        let headers = build_security_headers(&config);

        let hsts = headers
            .iter()
            .find(|(n, _)| n == STRICT_TRANSPORT_SECURITY)
            .map(|(_, v)| v.to_str().unwrap_or_default());

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
            .iter()
            .find(|(n, _)| n == X_FRAME_OPTIONS)
            .map(|(_, v)| v.to_str().unwrap_or_default());

        assert_eq!(frame_options, Some("SAMEORIGIN"));
    }
}
