//! GraphQL API tests using TestAppBuilder.
//!
//! These tests verify the GraphQL endpoint using the shared app builder.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use tower::ServiceExt;

#[tokio::test]
async fn test_graphql_playground() {
    let app = TestAppBuilder::graphql_only().build();

    // Send a GET request to the /graphql endpoint
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

    // Verify the response
    assert_eq!(response.status(), StatusCode::OK);

    // Get the response body
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");

    // Check that the response contains HTML for the GraphQL playground
    assert!(body_str.contains("<title>GraphQL Playground</title>"));
}

#[tokio::test]
async fn test_graphql_build_info_query() {
    let app = TestAppBuilder::graphql_only().build();

    // GraphQL query for build info
    let query = r#"{"query": "{ buildInfo { version gitSha buildTime } }"}"#;

    // Send a POST request with the query
    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(query))
                .expect("request"),
        )
        .await
        .expect("response");

    // Verify the response
    assert_eq!(response.status(), StatusCode::OK);

    // Get the response body
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");

    // Check that the response contains build info data
    assert!(body_str.contains("buildInfo"));
    assert!(body_str.contains("version"));
    assert!(body_str.contains("gitSha"));
    assert!(body_str.contains("buildTime"));
}
