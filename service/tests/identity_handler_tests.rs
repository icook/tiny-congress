//! Identity handler integration tests -- signup flow with real DB.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::valid_signup_json;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

#[shared_runtime_test]
async fn test_signup_handler_success() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(valid_signup_json("signuptest")))
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
    assert!(body_str.contains("device_kid"));
}

#[shared_runtime_test]
async fn test_signup_handler_duplicate_username() {
    let db = isolated_db().await;

    // First signup succeeds
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = valid_signup_json("dupuser");
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
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second signup with same username fails
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = valid_signup_json("dupuser");
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
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("Username already taken"));
}
