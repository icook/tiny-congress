//! Integration tests for POST /verifiers/endorsements endpoint.

mod common;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

use common::app_builder::TestAppBuilder;
use common::factories::{build_authed_request, valid_signup_with_keys};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::{create_endorsement, has_endorsement};

/// Helper: sign up a user and return (keys, account_id).
async fn signup_user(
    app: &axum::Router,
    username: &str,
) -> (common::factories::SignupKeys, uuid::Uuid) {
    let (signup_json, keys) = valid_signup_with_keys(username);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let parsed: Value = serde_json::from_slice(&body).expect("json");
    let account_id = parsed["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");
    (keys, account_id)
}

#[shared_runtime_test]
async fn test_verifier_can_create_endorsement() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    // Sign up a verifier account and a target user
    let (verifier_keys, verifier_id) = signup_user(&app, "test-verifier").await;
    let (_user_keys, user_id) = signup_user(&app, "target-user").await;

    // Bootstrap verifier endorsement (genesis)
    create_endorsement(
        db.pool(),
        verifier_id,
        "authorized_verifier",
        None,
        None,
        1.0,
        None,
    )
    .await
    .expect("bootstrap");

    // Call POST /verifiers/endorsements
    let body = json!({
        "username": "target-user",
        "topic": "identity_verified"
    })
    .to_string();

    let request = build_authed_request(
        Method::POST,
        "/verifiers/endorsements",
        &body,
        &verifier_keys.device_signing_key,
        &verifier_keys.device_kid,
    );

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Verify endorsement was created
    let has = has_endorsement(db.pool(), user_id, "identity_verified")
        .await
        .expect("check");
    assert!(has);
}

#[shared_runtime_test]
async fn test_non_verifier_gets_403() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    let (keys, _) = signup_user(&app, "regular-user").await;
    let _ = signup_user(&app, "target-user").await;

    let body = json!({
        "username": "target-user",
        "topic": "identity_verified"
    })
    .to_string();

    let request = build_authed_request(
        Method::POST,
        "/verifiers/endorsements",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[shared_runtime_test]
async fn test_endorsement_unknown_user_returns_404() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    let (verifier_keys, verifier_id) = signup_user(&app, "test-verifier").await;

    // Bootstrap verifier
    create_endorsement(
        db.pool(),
        verifier_id,
        "authorized_verifier",
        None,
        None,
        1.0,
        None,
    )
    .await
    .expect("bootstrap");

    let body = json!({
        "username": "nonexistent-user",
        "topic": "identity_verified"
    })
    .to_string();

    let request = build_authed_request(
        Method::POST,
        "/verifiers/endorsements",
        &body,
        &verifier_keys.device_signing_key,
        &verifier_keys.device_kid,
    );

    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_duplicate_endorsement_is_idempotent() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    let (verifier_keys, verifier_id) = signup_user(&app, "test-verifier").await;
    let _ = signup_user(&app, "target-user").await;

    create_endorsement(
        db.pool(),
        verifier_id,
        "authorized_verifier",
        None,
        None,
        1.0,
        None,
    )
    .await
    .expect("bootstrap");

    let body = json!({
        "username": "target-user",
        "topic": "identity_verified"
    })
    .to_string();

    // First call — should succeed
    let request = build_authed_request(
        Method::POST,
        "/verifiers/endorsements",
        &body,
        &verifier_keys.device_signing_key,
        &verifier_keys.device_kid,
    );
    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second call — same verifier, same subject+topic → idempotent upsert, returns 201
    let request = build_authed_request(
        Method::POST,
        "/verifiers/endorsements",
        &body,
        &verifier_keys.device_signing_key,
        &verifier_keys.device_kid,
    );
    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
}
