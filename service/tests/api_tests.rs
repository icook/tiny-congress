use async_graphql::{EmptySubscription, Schema};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::get,
    Extension, Router,
};
use tinycongress_api::build_info::BuildInfoProvider;
use tinycongress_api::graphql::{graphql_handler, graphql_playground, MutationRoot, QueryRoot};
use tower::ServiceExt;

fn create_test_app() -> Router {
    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(BuildInfoProvider::from_env())
        .finish();

    Router::new()
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        .layer(Extension(schema))
}

#[tokio::test]
async fn test_graphql_playground() {
    let app = create_test_app();

    // Send a GET request to the /graphql endpoint
    let response = app
        .oneshot(
            Request::builder()
                .uri("/graphql")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
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
async fn test_graphql_build_info_query() {
    let app = create_test_app();

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
                .unwrap(),
        )
        .await
        .unwrap();

    // Verify the response
    assert_eq!(response.status(), StatusCode::OK);

    // Get the response body
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // Check that the response contains build info data
    assert!(body_str.contains("buildInfo"));
    assert!(body_str.contains("version"));
    assert!(body_str.contains("gitSha"));
    assert!(body_str.contains("buildTime"));
}
