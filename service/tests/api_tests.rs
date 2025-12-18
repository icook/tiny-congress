//! GraphQL API tests using TestAppBuilder.
//!
//! These tests verify the GraphQL endpoint using the shared app builder with
//! structured JSON assertions for response validation.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use serde_json::Value;
use tower::ServiceExt;

// ============================================================================
// Test Helpers
// ============================================================================

/// Helper to execute a GraphQL POST request and parse the JSON response.
async fn graphql_post(query: &str) -> (StatusCode, Value) {
    let app = TestAppBuilder::graphql_only().build();
    let body = serde_json::json!({ "query": query }).to_string();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: Value = serde_json::from_slice(&body_bytes).expect("Response should be valid JSON");

    (status, json)
}

/// Helper to extract the "data" field from a GraphQL response.
fn extract_data(response: &Value) -> Option<&Value> {
    response.get("data").filter(|v| !v.is_null())
}

/// Helper to extract errors from a GraphQL response.
fn extract_errors(response: &Value) -> &[Value] {
    response
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
}

/// Assert that a GraphQL response has no errors.
fn assert_no_errors(response: &Value) {
    let errors = extract_errors(response);
    assert!(
        errors.is_empty(),
        "Expected no GraphQL errors, but got: {:?}",
        errors
    );
}

/// Assert that a GraphQL response contains at least one error.
fn assert_has_errors(response: &Value) {
    let errors = extract_errors(response);
    assert!(
        !errors.is_empty(),
        "Expected GraphQL errors, but response had none"
    );
}

// ============================================================================
// Basic Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_graphql_playground() {
    let app = TestAppBuilder::graphql_only().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("GET")
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

    // String assertion is appropriate here since this is HTML, not JSON
    assert!(
        body_str.contains("<title>GraphQL Playground</title>"),
        "Response should contain GraphQL Playground HTML"
    );
}

#[tokio::test]
async fn test_graphql_build_info_query() {
    let (status, json) = graphql_post("{ buildInfo { version gitSha buildTime } }").await;

    assert_eq!(status, StatusCode::OK);
    assert_no_errors(&json);

    let data = extract_data(&json).expect("Response should have data");
    let build_info = &data["buildInfo"];

    // Assert field types - these will fail on schema drift
    assert!(build_info.is_object(), "buildInfo should be an object");
    assert!(
        build_info["version"].is_string(),
        "version should be a string"
    );
    assert!(
        build_info["gitSha"].is_string(),
        "gitSha should be a string"
    );
    assert!(
        build_info["buildTime"].is_string(),
        "buildTime should be a string"
    );

    // Assert values are non-empty
    assert!(
        !build_info["version"]
            .as_str()
            .expect("version should be string")
            .is_empty(),
        "version should not be empty"
    );
    assert!(
        !build_info["gitSha"]
            .as_str()
            .expect("gitSha should be string")
            .is_empty(),
        "gitSha should not be empty"
    );
    assert!(
        !build_info["buildTime"]
            .as_str()
            .expect("buildTime should be string")
            .is_empty(),
        "buildTime should not be empty"
    );
}

#[tokio::test]
async fn test_graphql_echo_mutation() {
    let (status, json) = graphql_post(r#"mutation { echo(message: "test message") }"#).await;

    assert_eq!(status, StatusCode::OK);
    assert_no_errors(&json);

    let data = extract_data(&json).expect("Response should have data");
    assert_eq!(
        data["echo"].as_str().expect("echo should be string"),
        "test message",
        "echo should return the exact input"
    );
}

// ============================================================================
// HTTP-Level Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_graphql_invalid_json() {
    let app = TestAppBuilder::graphql_only().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from("{ invalid json }"))
                .expect("request"),
        )
        .await
        .expect("response");

    // async-graphql returns 400 for invalid JSON
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Invalid JSON should return 400"
    );
}

#[tokio::test]
async fn test_graphql_missing_query_field() {
    let app = TestAppBuilder::graphql_only().build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{ "variables": {} }"#))
                .expect("request"),
        )
        .await
        .expect("response");

    // Should return error status
    assert!(
        response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::OK,
        "Missing query should return error status or GraphQL error"
    );
}

#[tokio::test]
async fn test_graphql_error_response_structure() {
    let (status, json) = graphql_post("{ unknownField }").await;

    // HTTP status may still be 200 for GraphQL errors (per spec)
    assert_eq!(status, StatusCode::OK);

    // But response should have errors
    assert_has_errors(&json);

    // Verify error structure
    let errors = json["errors"]
        .as_array()
        .expect("errors should be an array");
    assert!(!errors.is_empty(), "Should have at least one error");

    // Each error should have a message
    for error in errors {
        assert!(
            error["message"].is_string(),
            "Each error should have a message field"
        );
    }
}

#[tokio::test]
async fn test_graphql_partial_success_not_possible_for_simple_query() {
    let (status, json) = graphql_post("{ buildInfo { version } }").await;

    assert_eq!(status, StatusCode::OK);

    // Should have data and no errors for valid query
    assert_no_errors(&json);
    assert!(
        extract_data(&json).is_some(),
        "Valid query should return data"
    );
}

#[tokio::test]
async fn test_graphql_content_type_json() {
    let app = TestAppBuilder::graphql_only().build();
    let body = serde_json::json!({ "query": "{ buildInfo { version } }" }).to_string();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    // Verify response content-type is JSON
    let content_type = response
        .headers()
        .get("content-type")
        .expect("Should have content-type header")
        .to_str()
        .expect("Content-type should be valid string");

    // async-graphql uses the GraphQL-over-HTTP spec compliant content type
    assert!(
        content_type.contains("application/json")
            || content_type.contains("application/graphql-response+json"),
        "Response should be JSON content type, got: {}",
        content_type
    );
}
