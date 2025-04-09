use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    Router,
    routing::get,
    Extension,
};
use async_graphql::{EmptySubscription, Schema};
use tinycongress_api::graphql::{QueryRoot, MutationRoot, graphql_playground, graphql_handler};
use tower::ServiceExt;

fn create_test_app() -> Router {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription).finish();
    
    Router::new()
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        .layer(Extension(schema))
}

#[tokio::test]
async fn test_graphql_playground() {
    let app = create_test_app();

    // Send a GET request to the /graphql endpoint
    let response = app
        .oneshot(Request::builder()
            .uri("/graphql")
            .method("GET")
            .body(Body::empty())
            .unwrap())
        .await
        .unwrap();

    // Verify the response
    assert_eq!(response.status(), StatusCode::OK);
    
    // Get the response body
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    
    // Check that the response contains HTML for the GraphQL playground
    assert!(body_str.contains("<title>GraphQL Playground</title>"));
}

#[tokio::test]
async fn test_graphql_query() {
    let app = create_test_app();

    // Simple GraphQL query
    let query = r#"{"query": "{ currentRound { id status } }"}"#;

    // Send a POST request with the query
    let response = app
        .oneshot(Request::builder()
            .uri("/graphql")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(query))
            .unwrap())
        .await
        .unwrap();

    // Verify the response
    assert_eq!(response.status(), StatusCode::OK);
    
    // Get the response body
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    
    // Check that the response contains the expected data
    assert!(body_str.contains("currentRound"));
    assert!(body_str.contains("id"));
    assert!(body_str.contains("status"));
    assert!(body_str.contains("active"));
}