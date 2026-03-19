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

// ─── Accept Invite auto-endorsement ──────────────────────────────────────────

#[shared_runtime_test]
async fn test_accept_invite_auto_enqueues_endorsement() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Sign up endorser
    let (app, endorser_keys, endorser_id) =
        signup_and_get_account("inviteendorser", db.pool()).await;

    // Sign up acceptor
    let (json2, acceptor_keys) = valid_signup_with_keys("inviteacceptor");
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
    let acceptor_id: uuid::Uuid = j2["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    // Endorser creates an invite
    let envelope_bytes = b"signed-invite-envelope";
    let envelope_b64 = tc_crypto::encode_base64url(envelope_bytes);
    let invite_body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": { "note": "auto-endorse test" }
    })
    .to_string();

    let create_req = build_authed_request(
        Method::POST,
        "/trust/invites",
        &invite_body,
        &endorser_keys.device_signing_key,
        &endorser_keys.device_kid,
    );
    let create_resp = app
        .clone()
        .oneshot(create_req)
        .await
        .expect("create response");
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    let create_json = json_body(create_resp).await;
    let invite_id = create_json["id"].as_str().expect("invite id");

    // Acceptor accepts the invite
    let accept_uri = format!("/trust/invites/{invite_id}/accept");
    let accept_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let accept_resp = app
        .clone()
        .oneshot(accept_req)
        .await
        .expect("accept response");
    assert_eq!(accept_resp.status(), StatusCode::OK);

    let accept_json = json_body(accept_resp).await;
    assert_eq!(
        accept_json["endorser_id"].as_str().expect("endorser_id"),
        endorser_id.to_string()
    );

    // Assert a pending endorsement action exists for the endorser
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(pool);
    let pending = trust_repo
        .claim_pending_actions(10)
        .await
        .expect("claim_pending_actions");

    let endorse_action = pending.iter().find(|a| {
        a.actor_id == endorser_id
            && a.action_type == "endorse"
            && a.payload["subject_id"]
                .as_str()
                .map(|s| s == acceptor_id.to_string())
                .unwrap_or(false)
    });

    assert!(
        endorse_action.is_some(),
        "expected a pending endorse action for endorser={endorser_id} subject={acceptor_id}, \
         found actions: {pending:?}"
    );

    let _ = (endorser_keys, acceptor_keys); // suppress unused warnings
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

// ─── Create invite validation ─────────────────────────────────────────────────

#[shared_runtime_test]
async fn create_invite_rejects_invalid_delivery_method() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("invitedelivery", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "fax",
        "attestation": {}
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(json["error"]
        .as_str()
        .unwrap_or("")
        .contains("delivery_method"));
}

#[shared_runtime_test]
async fn create_invite_rejects_invalid_relationship_depth() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("invitedepth", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "relationship_depth": "decades",
        "attestation": {}
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(json["error"]
        .as_str()
        .unwrap_or("")
        .contains("relationship_depth"));
}

// ─── Create invite weight validation ─────────────────────────────────────────

#[shared_runtime_test]
async fn create_invite_rejects_weight_zero() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("inviteweightzero", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "weight": 0.0,
        "attestation": {}
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(json["error"].as_str().unwrap_or("").contains("weight"));
}

#[shared_runtime_test]
async fn create_invite_rejects_weight_above_one() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("inviteweightabove", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "weight": 1.5,
        "attestation": {}
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(json["error"].as_str().unwrap_or("").contains("weight"));
}

// ─── Denounce validation ──────────────────────────────────────────────────────

#[shared_runtime_test]
async fn denounce_rejects_empty_reason() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("denouncereason1", db.pool()).await;

    let (json2, _) = valid_signup_with_keys("denounceetarget1");
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
        "reason": ""
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(json["error"].as_str().unwrap_or("").contains("reason"));
}

#[shared_runtime_test]
async fn denounce_rejects_reason_too_long() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("denouncereason2", db.pool()).await;

    let (json2, _) = valid_signup_with_keys("denounceetarget2");
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
        "reason": "a".repeat(501)
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(json["error"].as_str().unwrap_or("").contains("reason"));
}

// ─── Endorse after denounce ───────────────────────────────────────────────────

#[shared_runtime_test]
async fn endorse_after_denounce_returns_409() {
    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, keys, account_id) = signup_and_get_account("conflictendorser", db.pool()).await;

    // Sign up a target user
    let (json2, _) = valid_signup_with_keys("conflicttarget");
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
    let target_id: uuid::Uuid = j2["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    // Seed a denouncement row directly so has_active_denouncement returns true
    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(account_id)
    .bind(target_id)
    .bind("prior misbehavior")
    .execute(&pool)
    .await
    .expect("seed denouncement");

    // Attempt to endorse the denounced user — should be rejected with 409
    let body = serde_json::json!({ "subject_id": target_id }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
