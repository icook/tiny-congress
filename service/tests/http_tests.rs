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
use std::sync::Arc;
use tinycongress_api::config::SecurityHeadersConfig;
use tinycongress_api::identity::repo::{mock::MockAccountRepo, AccountRepoError};
use tower::ServiceExt;

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

#[tokio::test]
async fn test_identity_signup_success() {
    let app = TestAppBuilder::new()
        .with_identity_mocks()
        .with_health()
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"username": "testuser", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("account_id"));
    assert!(body_str.contains("root_kid"));
}

#[tokio::test]
async fn test_identity_signup_empty_username() {
    let app = TestAppBuilder::new()
        .with_identity_mocks()
        .with_health()
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"username": "", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                ))
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

#[tokio::test]
async fn test_identity_signup_duplicate_username() {
    let mock_repo = Arc::new(MockAccountRepo::new());
    mock_repo.set_create_result(Err(AccountRepoError::DuplicateUsername));

    let app = TestAppBuilder::new()
        .with_identity(mock_repo)
        .with_health()
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"username": "alice", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("Username already taken"));
}

#[tokio::test]
async fn test_identity_signup_invalid_pubkey() {
    let app = TestAppBuilder::new()
        .with_identity_mocks()
        .with_health()
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"username": "alice", "root_pubkey": "not-valid-base64url!!!"}"#,
                ))
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

    // Identity signup
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"username": "fulltest", "root_pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
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
