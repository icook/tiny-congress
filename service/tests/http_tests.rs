//! HTTP integration tests using TestAppBuilder.
//!
//! These tests verify the full HTTP layer including CORS, security headers,
//! identity routes, and GraphQL error propagation using the shared app builder
//! that mirrors main.rs wiring.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_SECURITY_POLICY,
            CONTENT_TYPE, ORIGIN, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, X_XSS_PROTECTION,
        },
        HeaderValue, Method, Request, StatusCode,
    },
};
use common::app_builder::TestAppBuilder;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::{encode_base64url, BackupEnvelope};
use tinycongress_api::config::SecurityHeadersConfig;
use tower::ServiceExt;

/// Build a valid signup request body with real Ed25519 keys and certificate.
fn valid_signup_body() -> String {
    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&certificate_sig.to_bytes());

    let envelope = BackupEnvelope::build(
        [0xAA; 16], // salt
        65536,
        3,
        1,           // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    format!(
        r#"{{"username": "testuser", "root_pubkey": "{root_pubkey}", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Test Device", "certificate": "{certificate}"}}}}"#
    )
}

// =============================================================================
// Health Check Tests
// =============================================================================

#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let app = TestAppBuilder::minimal().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_endpoint_with_full_app() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
}

// =============================================================================
// CORS Tests
// =============================================================================

#[tokio::test]
async fn test_cors_allows_configured_origin() {
    let app = TestAppBuilder::minimal()
        .with_cors(&["http://localhost:3000"])
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "http://localhost:3000")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    // Preflight should succeed
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static("http://localhost:3000"))
    );
}

#[tokio::test]
async fn test_cors_blocks_unconfigured_origin() {
    let app = TestAppBuilder::minimal()
        .with_cors(&["http://localhost:3000"])
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "http://evil.com")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    // Origin header should not be present for blocked origins
    assert!(response
        .headers()
        .get(ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());
}

#[tokio::test]
async fn test_cors_wildcard_allows_any_origin() {
    let app = TestAppBuilder::minimal().with_cors(&["*"]).build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "http://any-origin.com")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static("*"))
    );
}

#[tokio::test]
async fn test_cors_allows_multiple_origins() {
    let app = TestAppBuilder::minimal()
        .with_cors(&["http://localhost:3000", "https://app.example.com"])
        .build();

    // First origin
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "http://localhost:3000")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static("http://localhost:3000"))
    );

    // Second origin
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "https://app.example.com")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static("https://app.example.com"))
    );
}

#[tokio::test]
async fn test_cors_allows_configured_methods() {
    let app = TestAppBuilder::minimal()
        .with_cors(&["http://localhost:3000"])
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "http://localhost:3000")
                .header("Access-Control-Request-Method", "POST")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let methods = response
        .headers()
        .get(ACCESS_CONTROL_ALLOW_METHODS)
        .expect("should have allow-methods header");

    // Check that GET, POST, OPTIONS are allowed (matches main.rs config)
    let methods_str = methods.to_str().expect("valid string");
    assert!(
        methods_str.contains("GET") || methods_str.contains("get"),
        "should allow GET"
    );
    assert!(
        methods_str.contains("POST") || methods_str.contains("post"),
        "should allow POST"
    );
}

// =============================================================================
// Security Headers Tests
// =============================================================================

#[tokio::test]
async fn test_security_headers_default_config() {
    let app = TestAppBuilder::minimal()
        .with_security_headers_default()
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    // X-Content-Type-Options: nosniff
    assert_eq!(
        response.headers().get(X_CONTENT_TYPE_OPTIONS),
        Some(&HeaderValue::from_static("nosniff"))
    );

    // X-Frame-Options: DENY (default)
    assert_eq!(
        response.headers().get(X_FRAME_OPTIONS),
        Some(&HeaderValue::from_static("DENY"))
    );

    // X-XSS-Protection: 1; mode=block
    assert_eq!(
        response.headers().get(X_XSS_PROTECTION),
        Some(&HeaderValue::from_static("1; mode=block"))
    );

    // Content-Security-Policy: default-src 'self'
    assert_eq!(
        response.headers().get(CONTENT_SECURITY_POLICY),
        Some(&HeaderValue::from_static("default-src 'self'"))
    );
}

#[tokio::test]
async fn test_security_headers_custom_frame_options() {
    let mut config = SecurityHeadersConfig::default();
    config.frame_options = "SAMEORIGIN".to_string();

    let app = TestAppBuilder::minimal()
        .with_security_headers(config)
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.headers().get(X_FRAME_OPTIONS),
        Some(&HeaderValue::from_static("SAMEORIGIN"))
    );
}

#[tokio::test]
async fn test_security_headers_disabled() {
    let mut config = SecurityHeadersConfig::default();
    config.enabled = false;

    let app = TestAppBuilder::minimal()
        .with_security_headers(config)
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    // No security headers should be present
    assert!(response.headers().get(X_FRAME_OPTIONS).is_none());
    assert!(response.headers().get(X_CONTENT_TYPE_OPTIONS).is_none());
}

// =============================================================================
// Identity Routes Tests (with mocks)
// =============================================================================

// Note: test_identity_signup_success is covered by identity_handler_tests.rs since the
// transaction-based handler requires a real Postgres connection.

#[tokio::test]
async fn test_identity_signup_empty_username() {
    let app = TestAppBuilder::new()
        .with_identity_lazy()
        .with_health()
        .build();

    let body = valid_signup_body().replace("testuser", "");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("Username cannot be empty"));
}

// Note: test_identity_signup_duplicate_username is covered by identity_handler_tests.rs since
// the transaction-based handler requires a real Postgres connection.

#[tokio::test]
async fn test_identity_signup_invalid_pubkey() {
    let app = TestAppBuilder::new()
        .with_identity_lazy()
        .with_health()
        .build();

    // Replace the valid root_pubkey with garbage — the whole JSON structure must
    // still be valid so we construct it manually
    let device_pubkey = encode_base64url(&[2u8; 32]);
    let certificate = encode_base64url(&[3u8; 64]);
    let envelope = BackupEnvelope::build(
        [0xAA; 16], // salt
        65536,
        3,
        1,           // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    let body = format!(
        r#"{{"username": "alice", "root_pubkey": "not-valid-base64url!!!", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Test", "certificate": "{certificate}"}}}}"#
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid base64url"));
}

// =============================================================================
// GraphQL Tests
// =============================================================================

#[tokio::test]
async fn test_graphql_playground_accessible() {
    let app = TestAppBuilder::graphql_only().build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/graphql")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("GraphQL Playground"));
}

#[tokio::test]
async fn test_graphql_query_success() {
    let app = TestAppBuilder::graphql_only().build();

    let query = r#"{"query": "{ buildInfo { version gitSha buildTime } }"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/graphql")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(query))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("buildInfo"));
    assert!(body_str.contains("version"));
}

#[tokio::test]
async fn test_graphql_mutation_success() {
    let app = TestAppBuilder::graphql_only().build();

    let mutation = r#"{"query": "mutation { echo(message: \"hello world\") }"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/graphql")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(mutation))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("hello world"));
}

#[tokio::test]
async fn test_graphql_syntax_error_propagation() {
    let app = TestAppBuilder::graphql_only().build();

    // Invalid GraphQL syntax
    let query = r#"{"query": "{ invalid query syntax {"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/graphql")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(query))
                .expect("request"),
        )
        .await
        .expect("response");

    // GraphQL errors return 200 with error in body (per spec)
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("errors"));
}

#[tokio::test]
async fn test_graphql_unknown_field_error() {
    let app = TestAppBuilder::graphql_only().build();

    let query = r#"{"query": "{ nonExistentField }"}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/graphql")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(query))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("errors"));
    assert!(body_str.contains("nonExistentField"));
}

// =============================================================================
// REST API Tests
// =============================================================================

#[tokio::test]
async fn test_rest_build_info_endpoint() {
    let app = TestAppBuilder::new().with_rest().with_health().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/build-info")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("version"));
    assert!(body_str.contains("gitSha"));
}

// =============================================================================
// Full Stack Integration Tests
// =============================================================================

#[tokio::test]
async fn test_full_app_all_routes_accessible() {
    let app = TestAppBuilder::with_mocks().build();

    // Health check
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // GraphQL playground
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // REST API
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/build-info")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // Identity signup route is accessible (validation error proves routing works;
    // full signup success is tested in identity_handler_tests.rs with a real Postgres connection)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"username": "", "root_pubkey": "x", "backup": {"encrypted_blob": "x"}, "device": {"pubkey": "x", "name": "x", "certificate": "x"}}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    // BAD_REQUEST (not 404) proves the route is mounted and the handler runs
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_full_app_has_security_headers() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    // Verify security headers are present (with_mocks enables them)
    assert_eq!(
        response.headers().get(X_CONTENT_TYPE_OPTIONS),
        Some(&HeaderValue::from_static("nosniff"))
    );
    assert_eq!(
        response.headers().get(X_FRAME_OPTIONS),
        Some(&HeaderValue::from_static("DENY"))
    );
}

#[tokio::test]
async fn test_full_app_has_cors() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header(ORIGIN, "http://localhost:3000")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static("http://localhost:3000"))
    );
}

/// Comprehensive production-like integration test.
///
/// This test verifies the full application stack works correctly with all
/// features enabled, similar to how the production server runs. It checks:
/// - All routes are accessible (health, GraphQL, REST, identity, swagger)
/// - Security headers are applied correctly
/// - CORS is properly configured
/// - GraphQL queries execute successfully
/// - REST endpoints return valid responses
#[tokio::test]
async fn test_production_like_full_stack() {
    let app = TestAppBuilder::with_mocks().build();

    // 1. Health check endpoint works
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    // Verify security headers on health response
    assert_eq!(
        response.headers().get(X_CONTENT_TYPE_OPTIONS),
        Some(&HeaderValue::from_static("nosniff"))
    );
    assert_eq!(
        response.headers().get(X_FRAME_OPTIONS),
        Some(&HeaderValue::from_static("DENY"))
    );
    assert_eq!(
        response.headers().get(X_XSS_PROTECTION),
        Some(&HeaderValue::from_static("1; mode=block"))
    );
    assert_eq!(
        response.headers().get(CONTENT_SECURITY_POLICY),
        Some(&HeaderValue::from_static("default-src 'self'"))
    );

    // 2. GraphQL query executes correctly
    let query = r#"{"query": "{ buildInfo { version gitSha buildTime } }"}"#;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/graphql")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(query))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("buildInfo"));
    assert!(!body_str.contains("errors"));

    // 3. REST API returns valid JSON
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/build-info")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("version"));
    assert!(body_str.contains("gitSha"));

    // 4. Identity signup route is mounted (validation error proves routing;
    //    full signup tested in identity_handler_tests.rs with real Postgres)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"username": "", "root_pubkey": "x", "backup": {"encrypted_blob": "x"}, "device": {"pubkey": "x", "name": "x", "certificate": "x"}}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 5. Swagger UI is accessible
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/swagger-ui/")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // 6. CORS preflight works for configured origin
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/graphql")
                .header(ORIGIN, "http://localhost:3000")
                .header("Access-Control-Request-Method", "POST")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&HeaderValue::from_static("http://localhost:3000"))
    );
}
