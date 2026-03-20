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

// ─── Endorse self-action validation ──────────────────────────────────────────

#[shared_runtime_test]
async fn endorse_rejects_self_endorsement() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("selfendorser", db.pool()).await;

    let body = serde_json::json!({ "subject_id": account_id, "weight": 1.0 }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
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
        .to_lowercase()
        .contains("yourself"));
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
    use tinycongress_api::trust::repo::ActionRecord;
    let pending = sqlx::query_as::<_, ActionRecord>(
        "SELECT * FROM trust__action_log WHERE status = 'pending' ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .expect("query pending actions");

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
async fn create_invite_rejects_invalid_base64url_envelope() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("invitebadb64", db.pool()).await;

    let body = serde_json::json!({
        "envelope": "not!!valid%%base64url",
        "delivery_method": "qr",
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
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("base64"),
        "error should mention base64, got: {}",
        json["error"]
    );
}

#[shared_runtime_test]
async fn create_invite_rejects_oversized_envelope() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("invitebigenvelop", db.pool()).await;

    // 4097 bytes exceeds the 4096-byte maximum
    let oversized = vec![0u8; 4097];
    let envelope_b64 = tc_crypto::encode_base64url(&oversized);
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
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
    assert!(
        json["error"].as_str().unwrap_or("").contains("envelope"),
        "error should mention envelope, got: {}",
        json["error"]
    );
}

#[shared_runtime_test]
async fn create_invite_rejects_empty_envelope() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("inviteemptyenv", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(&[]);
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
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
    assert!(
        json["error"].as_str().unwrap_or("").contains("envelope"),
        "error should mention envelope, got: {}",
        json["error"]
    );
}

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

// ─── Denounce self-action validation ─────────────────────────────────────────

#[shared_runtime_test]
async fn denounce_rejects_self_denouncement() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("selfdenouncer", db.pool()).await;

    let body = serde_json::json!({
        "target_id": account_id,
        "reason": "testing self denouncement"
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
    assert!(json["error"]
        .as_str()
        .unwrap_or("")
        .to_lowercase()
        .contains("yourself"));
}

// ─── List denouncements ───────────────────────────────────────────────────────

#[shared_runtime_test]
async fn list_my_denouncements_returns_denouncement_with_username() {
    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, keys, account_id) = signup_and_get_account("denouncerlister", db.pool()).await;

    // Sign up a target so the JOIN on accounts succeeds and returns a username
    let (json2, _) = valid_signup_with_keys("denounceelisted");
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

    // Seed a denouncement row directly so it shows up immediately in the list
    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(account_id)
    .bind(target_id)
    .bind("spam behavior")
    .execute(&pool)
    .await
    .expect("seed denouncement");

    let request = build_authed_request(
        Method::GET,
        "/trust/denouncements/mine",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    let denouncements = json.as_array().expect("denouncements array");
    assert_eq!(denouncements.len(), 1);
    assert_eq!(
        denouncements[0]["target_id"].as_str().unwrap(),
        target_id.to_string()
    );
    assert_eq!(
        denouncements[0]["reason"].as_str().unwrap(),
        "spam behavior"
    );
    // Verify the JOIN on accounts returned the target's username
    assert!(
        denouncements[0]["target_username"].is_string(),
        "target_username should be present from JOIN on accounts"
    );
    assert_eq!(
        denouncements[0]["target_username"].as_str().unwrap(),
        "denounceelisted"
    );
}

// ─── Accept invite — error paths ─────────────────────────────────────────────

/// The endorser who created an invite must not be able to accept it themselves.
/// Without this guard they could permanently consume the invite token, preventing
/// the intended recipient from ever accepting it.
#[shared_runtime_test]
async fn accept_invite_rejects_self_accept() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("selfacceptendorser", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy-envelope");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": {}
    })
    .to_string();

    let create_req = build_authed_request(
        Method::POST,
        "/trust/invites",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let create_resp = app.clone().oneshot(create_req).await.expect("create");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let invite_json = json_body(create_resp).await;
    let invite_id = invite_json["id"].as_str().expect("invite id");

    // Endorser attempts to accept their own invite — must be rejected.
    let accept_uri = format!("/trust/invites/{invite_id}/accept");
    let accept_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let accept_resp = app.oneshot(accept_req).await.expect("accept");
    assert_eq!(accept_resp.status(), StatusCode::BAD_REQUEST);
    let json = json_body(accept_resp).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("own invite"),
        "error should mention 'own invite', got: {}",
        json["error"]
    );
}

#[shared_runtime_test]
async fn accept_invite_returns_404_for_nonexistent_invite() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("acceptnotfound", db.pool()).await;

    let fake_id = uuid::Uuid::new_v4();
    let uri = format!("/trust/invites/{fake_id}/accept");
    let request = build_authed_request(
        Method::POST,
        &uri,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Accepting an expired invite must return 404 — the SQL UPDATE's `expires_at > now()`
/// guard rejects it even though the invite row itself still exists in the DB.
#[shared_runtime_test]
async fn accept_invite_returns_404_when_expired() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Sign up an endorser and an acceptor.
    let (app, _endorser_keys, endorser_id) =
        signup_and_get_account("expiredendorser", db.pool()).await;

    let (json2, acceptor_keys) = valid_signup_with_keys("expiredacceptor");
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
    assert_eq!(resp2.status(), StatusCode::CREATED);

    // Insert an already-expired invite directly via SQL, bypassing the HTTP
    // handler that always sets expires_at = now() + 7 days.
    let invite_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO trust__invites \
         (endorser_id, envelope, delivery_method, weight, attestation, expires_at) \
         VALUES ($1, $2, $3, $4, $5, now() - interval '1 hour') \
         RETURNING id",
    )
    .bind(endorser_id)
    .bind(b"dummy-envelope" as &[u8])
    .bind("qr")
    .bind(1.0_f32)
    .bind(serde_json::json!({}))
    .fetch_one(&pool)
    .await
    .expect("insert expired invite");

    // Acceptor attempts to accept the expired invite — must return 404.
    let accept_uri = format!("/trust/invites/{invite_id}/accept");
    let accept_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let accept_resp = app.oneshot(accept_req).await.expect("accept response");
    assert_eq!(accept_resp.status(), StatusCode::NOT_FOUND);

    let _ = (_endorser_keys, acceptor_keys);
}

/// Accepting an already-accepted invite must return 404 — the SQL UPDATE's
/// `accepted_by IS NULL` guard rejects it the same way as a missing invite.
#[shared_runtime_test]
async fn accept_invite_returns_404_when_already_accepted() {
    let db = isolated_db().await;

    // Sign up endorser and acceptor.
    let (app, endorser_keys, _endorser_id) =
        signup_and_get_account("alreadyacceptedendorser", db.pool()).await;

    let (json2, acceptor_keys) = valid_signup_with_keys("alreadyacceptedacceptor");
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
    assert_eq!(resp2.status(), StatusCode::CREATED);

    // Endorser creates an invite.
    let envelope_b64 = tc_crypto::encode_base64url(b"signed-invite-envelope");
    let invite_body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": { "note": "double-accept test" }
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
    let invite_id = json_body(create_resp).await;
    let invite_id = invite_id["id"].as_str().expect("invite id");

    // First accept — must succeed.
    let accept_uri = format!("/trust/invites/{invite_id}/accept");
    let first_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let first_resp = app.clone().oneshot(first_req).await.expect("first accept");
    assert_eq!(first_resp.status(), StatusCode::OK);

    // Second accept — must return 404 because `accepted_by IS NULL` no longer matches.
    let second_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let second_resp = app
        .clone()
        .oneshot(second_req)
        .await
        .expect("second accept");
    assert_eq!(second_resp.status(), StatusCode::NOT_FOUND);

    let _ = (endorser_keys, acceptor_keys);
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

// ─── Denounce budget exhaustion ──────────────────────────────────────────────

/// When a user has used all denouncement slots (d=2), a third denounce attempt
/// must return 429 Too Many Requests.
#[shared_runtime_test]
async fn denounce_returns_429_when_budget_exhausted() {
    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, keys, account_id) = signup_and_get_account("denouncebudget", db.pool()).await;

    // Sign up two targets so we have valid UUIDs to reference.
    let (json_t1, _) = valid_signup_with_keys("budgettarget1");
    let resp_t1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json_t1))
                .expect("request"),
        )
        .await
        .expect("response");
    let body_t1 = axum::body::to_bytes(resp_t1.into_body(), 1024 * 1024)
        .await
        .expect("body_t1");
    let j_t1: Value = serde_json::from_slice(&body_t1).expect("json_t1");
    let target1_id: uuid::Uuid = j_t1["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    let (json_t2, _) = valid_signup_with_keys("budgettarget2");
    let resp_t2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json_t2))
                .expect("request"),
        )
        .await
        .expect("response");
    let body_t2 = axum::body::to_bytes(resp_t2.into_body(), 1024 * 1024)
        .await
        .expect("body_t2");
    let j_t2: Value = serde_json::from_slice(&body_t2).expect("json_t2");
    let target2_id: uuid::Uuid = j_t2["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    // Seed 2 denouncements directly to exhaust the d=2 budget without consuming
    // daily quota (which would trigger QuotaExceeded instead).
    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(account_id)
    .bind(target1_id)
    .bind("first")
    .execute(&pool)
    .await
    .expect("seed denouncement 1");

    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(account_id)
    .bind(target2_id)
    .bind("second")
    .execute(&pool)
    .await
    .expect("seed denouncement 2");

    // Sign up a third target and attempt to denounce — budget is exhausted.
    let (json_t3, _) = valid_signup_with_keys("budgettarget3");
    let resp_t3 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json_t3))
                .expect("request"),
        )
        .await
        .expect("response");
    let body_t3 = axum::body::to_bytes(resp_t3.into_body(), 1024 * 1024)
        .await
        .expect("body_t3");
    let j_t3: Value = serde_json::from_slice(&body_t3).expect("json_t3");
    let target3_id = j_t3["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({
        "target_id": target3_id,
        "reason": "third denouncement"
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
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ─── Accept invite — auto-endorse silent failure ──────────────────────────────

/// When the endorser's slots are full at the moment an invite is accepted,
/// accept_invite returns 200 OK (the invite IS accepted) but no endorsement
/// action is queued.  This documents the current fire-and-forget behaviour of
/// the auto-endorse step so that any future change to propagate the error is
/// caught by a test failure.
#[shared_runtime_test]
async fn accept_invite_succeeds_even_when_endorser_slots_exhausted() {
    use common::factories::{insert_endorsement, AccountFactory};

    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, endorser_keys, endorser_id) =
        signup_and_get_account("exhaustedendorser", db.pool()).await;

    // Fill the endorser's k=3 slots directly in the DB.
    for seed in 50u8..53 {
        let subject = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create dummy subject");
        insert_endorsement(&pool, endorser_id, subject.id, 1.0).await;
    }

    // Sign up the acceptor.
    let (json2, acceptor_keys) = valid_signup_with_keys("exhaustedacceptor");
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
    assert_eq!(resp2.status(), StatusCode::CREATED);

    // Endorser creates an invite.
    let envelope_b64 = tc_crypto::encode_base64url(b"signed-invite-envelope");
    let invite_body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": { "note": "slots exhausted test" }
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

    let invite_id = json_body(create_resp).await;
    let invite_id = invite_id["id"].as_str().expect("invite id");

    // Acceptor accepts the invite — 200 OK even though auto-endorse will fail.
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

    // An out-of-slot endorsement action should have been queued.
    let pending = sqlx::query_as::<_, tinycongress_api::trust::repo::ActionRecord>(
        "SELECT * FROM trust__action_log WHERE status = 'pending' ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .expect("query pending actions");
    let endorse_action = pending
        .iter()
        .find(|a| a.actor_id == endorser_id && a.action_type == "endorse");
    assert!(
        endorse_action.is_some(),
        "expected an out-of-slot endorse action to be queued, found none"
    );
    let in_slot = endorse_action
        .and_then(|a| a.payload["in_slot"].as_bool())
        .unwrap_or(true);
    assert!(
        !in_slot,
        "expected in_slot=false for out-of-slot endorsement"
    );

    let _ = (endorser_keys, acceptor_keys);
}

// ─── List invites ─────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn list_invites_returns_created_invite() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("listinvitecreator", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy-envelope-bytes");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "email",
        "attestation": { "note": "list test" }
    })
    .to_string();

    let create_req = build_authed_request(
        Method::POST,
        "/trust/invites",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let create_resp = app.clone().oneshot(create_req).await.expect("create");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created = json_body(create_resp).await;
    let invite_id = created["id"].as_str().expect("invite id");

    let list_req = build_authed_request(
        Method::GET,
        "/trust/invites/mine",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let list_resp = app.oneshot(list_req).await.expect("list");
    assert_eq!(list_resp.status(), StatusCode::OK);

    let json = json_body(list_resp).await;
    let invites = json["invites"].as_array().expect("invites array");
    assert_eq!(invites.len(), 1);
    assert_eq!(invites[0]["id"].as_str().unwrap(), invite_id);
    assert_eq!(invites[0]["delivery_method"].as_str().unwrap(), "email");
    assert!(invites[0]["accepted_by"].is_null());
}

// ─── Endorse beyond slot limit ────────────────────────────────────────────────

/// When a non-verifier user has used all k=3 endorsement slots, a direct
/// endorse request succeeds with 201 (endorsement is stored as out-of-slot).
/// This was changed in #754 — endorsements beyond the slot limit are allowed
/// but don't contribute to trust graph computation.
#[shared_runtime_test]
async fn endorse_succeeds_as_out_of_slot_when_slots_full() {
    use common::factories::{insert_endorsement, AccountFactory};

    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, keys, endorser_id) = signup_and_get_account("slotexhausted", db.pool()).await;

    // Fill the endorser's k=3 slots directly in the DB (bypasses daily quota).
    for seed in 50u8..53 {
        let subject = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create dummy subject");
        insert_endorsement(&pool, endorser_id, subject.id, 1.0).await;
    }

    // Sign up a 4th target and attempt to endorse — should succeed as out-of-slot.
    let (json4, _) = valid_signup_with_keys("slotexhaustedsubject4");
    let resp4 = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json4))
                .expect("request"),
        )
        .await
        .expect("response");
    let body4 = axum::body::to_bytes(resp4.into_body(), 1024 * 1024)
        .await
        .expect("body4");
    let j4: Value = serde_json::from_slice(&body4).expect("json4");
    let subject4_id = j4["account_id"].as_str().expect("account_id");

    let body = serde_json::json!({ "subject_id": subject4_id }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/endorse",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "4th endorsement should succeed as out-of-slot"
    );
}

// ─── Revoke self-action validation ───────────────────────────────────────────

#[shared_runtime_test]
async fn revoke_rejects_self_revocation() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("selfrevoke", db.pool()).await;

    let body = serde_json::json!({ "subject_id": account_id }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/revoke",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("yourself"),
        "error should mention 'yourself', got: {}",
        json["error"]
    );
}

// ─── Revoke quota exhaustion ──────────────────────────────────────────────────

/// When a user has exhausted the daily action quota, a revoke attempt must
/// return 429 Too Many Requests.
#[shared_runtime_test]
async fn revoke_returns_429_when_quota_exceeded() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("revokequota", db.pool()).await;

    // Seed 5 actions (daily quota) directly so we don't consume real API budget.
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(account_id, "endorse", &serde_json::json!({}))
            .await
            .expect("enqueue");
    }

    // Sign up a second user to revoke (subject_id must be a valid UUID).
    let (json2, _) = valid_signup_with_keys("revokequotasubject");
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
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ─── Denounce quota ───────────────────────────────────────────────────────────

// ─── Budget with out-of-slot endorsements ────────────────────────────────────

/// Verify that `slots_used` only counts in-slot endorsements and `out_of_slot_count`
/// correctly reflects endorsements stored beyond the k=3 slot limit.
/// The empty-state case is covered by `test_budget_returns_200`; this test exercises
/// the non-trivial `all_endorsements - endorsements_used` path.
#[shared_runtime_test]
async fn budget_correctly_reports_out_of_slot_count() {
    use common::factories::{insert_endorsement, AccountFactory};

    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, keys, endorser_id) = signup_and_get_account("budgetoutofslot", db.pool()).await;

    // Seed k=3 in-slot endorsements directly (default in_slot=true).
    for seed in 70u8..73 {
        let subject = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create in-slot subject");
        insert_endorsement(&pool, endorser_id, subject.id, 1.0).await;
    }

    // Seed 1 out-of-slot endorsement (in_slot=false).
    let oos_subject = AccountFactory::new()
        .with_seed(73u8)
        .create(&pool)
        .await
        .expect("create out-of-slot subject");
    sqlx::query(
        "INSERT INTO reputation__endorsements \
         (endorser_id, subject_id, topic, weight, in_slot) \
         VALUES ($1, $2, 'trust', 1.0, false)",
    )
    .bind(endorser_id)
    .bind(oos_subject.id)
    .execute(&pool)
    .await
    .expect("insert out-of-slot endorsement");

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
    assert_eq!(json["slots_total"], 3, "slots_total should equal k=3 limit");
    assert_eq!(
        json["slots_used"], 3,
        "slots_used should count only in-slot endorsements"
    );
    assert_eq!(
        json["slots_available"], 0,
        "slots_available should be 0 when all slots used"
    );
    assert_eq!(
        json["out_of_slot_count"], 1,
        "out_of_slot_count should reflect endorsements beyond the slot limit"
    );
}

#[shared_runtime_test]
async fn denounce_returns_429_when_quota_exceeded() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("denouncequota", db.pool()).await;

    // Seed 5 actions (daily quota) directly so we don't consume real API budget.
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(account_id, "endorse", &serde_json::json!({}))
            .await
            .expect("enqueue");
    }

    // Sign up a second user to denounce.
    let (json2, _) = valid_signup_with_keys("denouncequotasubject");
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

    let body = serde_json::json!({ "target_id": target_id, "reason": "spam" }).to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/denounce",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
