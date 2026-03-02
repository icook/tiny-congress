//! Login handler integration tests -- login flow with real DB.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::{valid_signup_with_keys, SignupKeys};
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::encode_base64url;
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

/// Sign up a user and return the app + keys for subsequent login requests.
async fn signup_user(username: &str) -> (axum::Router, SignupKeys, common::test_db::IsolatedDb) {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let (json, keys) = valid_signup_with_keys(username);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    (app, keys, db)
}

fn login_json(username: &str, keys: &SignupKeys) -> String {
    // Generate a new device key for login (different from signup device)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    serde_json::json!({
        "username": username,
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Login Device",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string()
}

#[shared_runtime_test]
async fn test_login_handler_success() {
    let (app, keys, _db) = signup_user("logintest").await;

    let body = login_json("logintest", &keys);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(resp_body.to_vec()).expect("utf8");
    assert!(body_str.contains("account_id"));
    assert!(body_str.contains("root_kid"));
    assert!(body_str.contains("device_kid"));
}

#[shared_runtime_test]
async fn test_login_handler_account_not_found() {
    let (app, keys, _db) = signup_user("loginnf").await;

    let body = login_json("nonexistent", &keys);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_login_handler_invalid_certificate() {
    let (app, _keys, _db) = signup_user("loginbadcert").await;

    // Generate a device key but sign with a random key (not the root)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let wrong_root = SigningKey::generate(&mut OsRng);
    let bad_cert = wrong_root.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "loginbadcert",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Bad Cert Device",
            "certificate": encode_base64url(&bad_cert.to_bytes()),
        }
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn test_login_handler_duplicate_device() {
    let (app, keys, _db) = signup_user("logindup").await;

    // Generate a device key and login with it
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "logindup",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "First Login",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Same device key again should conflict
    let body2 = serde_json::json!({
        "username": "logindup",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Second Login",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let response2 = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body2))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response2.status(), StatusCode::CONFLICT);
}

#[shared_runtime_test]
async fn test_login_handler_empty_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = serde_json::json!({
        "username": "   ",
        "device": {
            "pubkey": encode_base64url(&[1u8; 32]),
            "name": "Test Device",
            "certificate": encode_base64url(&[0u8; 64]),
        }
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
