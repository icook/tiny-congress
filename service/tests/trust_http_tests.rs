//! Integration tests for trust HTTP endpoints.
//!
//! Tests cover the full stack: HTTP → service → repo for all trust endpoints.

mod common;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use common::app_builder::TestAppBuilder;
use common::factories::{build_authed_request, valid_signup_with_keys};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;

/// Helper: sign up a user and return (app, keys, account_id).
async fn signup_and_get_account(
    username: &str,
    pool: &sqlx::PgPool,
) -> (axum::Router, common::factories::SignupKeys, uuid::Uuid) {
    let app = TestAppBuilder::new().with_trust_pool(pool.clone()).build();

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

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    let account_id: uuid::Uuid = json["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    (app, keys, account_id)
}

/// Helper: parse JSON response body.
async fn json_body(response: axum::http::Response<Body>) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    serde_json::from_slice(&body).expect("json")
}

// ─── Endorse ─────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_endorse_returns_202() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("endorser1", db.pool()).await;

    // Sign up a second user to endorse
    let (json2, _) = valid_signup_with_keys("endorsee1");
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let json2: Value = serde_json::from_slice(&body2).expect("json2");
    let subject_id = json2["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({ "subject_id": subject_id, "weight": 1.0 }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let json = json_body(response).await;
    assert_eq!(json["message"], "endorsement queued");
}

#[shared_runtime_test]
async fn test_endorse_self_returns_400() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("selfendorser", db.pool()).await;

    let body = serde_json::json!({ "subject_id": account_id }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn test_endorse_quota_exceeded_returns_429() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("quotauser", db.pool()).await;

    // Seed 5 actions (daily quota) directly in the DB
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(account_id, "endorse", &serde_json::json!({}))
            .await
            .expect("enqueue");
    }

    // Sign up another user
    let (json2, _) = valid_signup_with_keys("quotasubject");
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let j2: Value = serde_json::from_slice(&body2).expect("json2");
    let subject_id = j2["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({ "subject_id": subject_id }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ─── Revoke ───────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_revoke_returns_202() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("revoker1", db.pool()).await;

    // Sign up a user to revoke endorsement from
    let (json2, _) = valid_signup_with_keys("revokee1");
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let j2: Value = serde_json::from_slice(&body2).expect("json2");
    let subject_id = j2["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({ "subject_id": subject_id }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/revoke",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let json = json_body(response).await;
    assert_eq!(json["message"], "revocation queued");
}

// ─── Denounce ────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_denounce_returns_202() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("denouncer1", db.pool()).await;

    // Sign up a user to denounce
    let (json2, _) = valid_signup_with_keys("denouncee1");
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let j2: Value = serde_json::from_slice(&body2).expect("json2");
    let target_id = j2["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({
        "target_id": target_id,
        "reason": "spamming"
    })
    .to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/denounce",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let json = json_body(response).await;
    assert_eq!(json["message"], "denouncement queued");
}

// ─── Scores ───────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_scores_me_returns_200() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("scoreuser", db.pool()).await;

    // Seed a trust score snapshot
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    trust_repo
        .upsert_score(account_id, None, Some(1.0), Some(2), Some(0.5))
        .await
        .expect("upsert_score");

    let request = build_authed_request(
        Method::GET,
        "/trust/scores/me",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    let scores = json["scores"].as_array().expect("scores array");
    assert_eq!(scores.len(), 1);
    assert!(scores[0]["trust_distance"].as_f64().is_some());
}

// ─── Budget ───────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_budget_returns_200() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("budgetuser", db.pool()).await;

    let request = build_authed_request(
        Method::GET,
        "/trust/budget",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    assert_eq!(json["slots_total"], 3);
    assert_eq!(json["slots_used"], 0);
    assert_eq!(json["slots_available"], 3);
    assert_eq!(json["denouncements_total"], 2);
    assert_eq!(json["denouncements_used"], 0);
    assert_eq!(json["denouncements_available"], 2);
}

// ─── Invites ──────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_create_invite_returns_201() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("invitecreator", db.pool()).await;

    // base64url-encode some dummy envelope bytes
    let envelope_bytes = b"dummy-envelope-bytes";
    let envelope_b64 = tc_crypto::encode_base64url(envelope_bytes);

    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "email",
        "attestation": { "note": "test invite" }
    })
    .to_string();

    let request = build_authed_request(
        Method::POST,
        "/trust/invites",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = json_body(response).await;
    assert!(json["id"].is_string());
    assert!(json["expires_at"].is_string());
}

// ─── Endorse weight validation ────────────────────────────────────────────────

#[shared_runtime_test]
async fn endorse_rejects_weight_zero() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("weightzeroendorser", db.pool()).await;

    let (json2, _) = valid_signup_with_keys("weightzerosubject");
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let j2: Value = serde_json::from_slice(&body2).expect("json2");
    let subject_id = j2["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({ "subject_id": subject_id, "weight": 0.0 }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn endorse_rejects_weight_above_one() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("weightaboveendorser", db.pool()).await;

    let (json2, _) = valid_signup_with_keys("weightabovesubject");
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let j2: Value = serde_json::from_slice(&body2).expect("json2");
    let subject_id = j2["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({ "subject_id": subject_id, "weight": 1.5 }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
