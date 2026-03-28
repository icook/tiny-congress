//! Integration tests for trust HTTP endpoints.
//!
//! Tests cover the full stack: HTTP → service → repo for all trust endpoints.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use common::app_builder::TestAppBuilder;
use common::factories::{build_authed_request, valid_signup_with_keys};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::{
    CreatedEndorsement, EndorsementRecord, EndorsementRepoError, ExternalIdentityRecord,
    ExternalIdentityRepoError, ReputationRepo,
};
use tinycongress_api::trust::repo::{
    ActionRecord, DenouncementRecord, DenouncementWithUsername, InfluenceRecord, InviteRecord,
    ScoreSnapshot, TrustRepo, TrustRepoError,
};
use tinycongress_api::trust::service::{ActionType, TrustService, TrustServiceError};
use tinycongress_api::trust::weight::{DeliveryMethod, RelationshipDepth};

// ─── Stub TrustRepo for accepted_at=None scenario ────────────────────────────

/// Stub [`TrustRepo`] that returns an [`InviteRecord`] with `accepted_at = None`
/// from both `get_invite` and `accept_invite`.  All other methods panic — this
/// stub is only valid for the `accept_invite_handler` code path.
struct StubAcceptInviteNullTimestamp {
    endorser_id: Uuid,
}

impl StubAcceptInviteNullTimestamp {
    fn invite_record(&self) -> InviteRecord {
        InviteRecord {
            id: Uuid::new_v4(),
            endorser_id: self.endorser_id,
            envelope: vec![0u8],
            delivery_method: "qr".to_string(),
            attestation: serde_json::Value::Object(serde_json::Map::new()),
            accepted_by: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(7),
            accepted_at: None, // ← the invariant under test
            created_at: chrono::Utc::now(),
            relationship_depth: None,
            weight: 1.0,
        }
    }
}

#[async_trait]
impl TrustRepo for StubAcceptInviteNullTimestamp {
    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        Ok(self.invite_record())
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        Ok(self.invite_record())
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubAcceptInviteNullTimestamp: not needed for this test")
    }
}

/// Stub [`TrustService`] that panics on every call.
///
/// The `accept_invite_handler` returns early with 500 before reaching the
/// service call when `accepted_at` is `None`, so this stub should never
/// be invoked in the test below.
struct PanickingTrustService;

#[async_trait]
impl TrustService for PanickingTrustService {
    async fn endorse(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _weight: f32,
        _attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("PanickingTrustService: must not be called in this test")
    }
    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("PanickingTrustService: must not be called in this test")
    }
    async fn denounce(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("PanickingTrustService: must not be called in this test")
    }
}

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
async fn test_endorse_quota_exceeded_returns_429() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("quotauser", db.pool()).await;

    // Seed 5 actions (daily quota) directly in the DB
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    use tinycongress_api::trust::service::ActionType;
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(account_id, ActionType::Endorse, &serde_json::json!({}))
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

#[shared_runtime_test]
async fn test_endorse_denouncement_conflict_returns_409() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("conflictendorser", db.pool()).await;

    // Sign up the target user
    let (json2, _) = valid_signup_with_keys("conflictsubject");
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
    let subject_id: uuid::Uuid = j2["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    // Seed an active denouncement from endorser → subject
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    trust_repo
        .create_denouncement(account_id, subject_id, "test conflict")
        .await
        .expect("create_denouncement");

    // Attempting to endorse the denounced subject must return 409 Conflict
    let body = serde_json::json!({ "subject_id": subject_id }).to_string();
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

#[shared_runtime_test]
async fn test_revoke_self_returns_400() {
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

#[shared_runtime_test]
async fn test_denounce_already_denounced_returns_409() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("dupedenouncer", db.pool()).await;

    // Sign up the target user
    let (json2, _) = valid_signup_with_keys("dupedenouncesubject");
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

    // Seed an existing denouncement directly so the service sees AlreadyDenounced
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    trust_repo
        .create_denouncement(account_id, target_id, "prior denouncement")
        .await
        .expect("create_denouncement");

    // Attempting to denounce the same target again must return 409 Conflict
    let body = serde_json::json!({
        "target_id": target_id,
        "reason": "duplicate denouncement attempt"
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
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

// ─── Scores ───────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_scores_me_returns_200() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("scoreuser", db.pool()).await;

    // Seed a trust score snapshot
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    use tinycongress_api::trust::service::ActionType;
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

// ─── Endorse attestation size validation ─────────────────────────────────────

#[shared_runtime_test]
async fn endorse_rejects_oversized_attestation() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("endorseoversizedatt", db.pool()).await;

    // Use any UUID — attestation size validation fires before any DB call.
    let subject_id = uuid::Uuid::new_v4();
    // A string value of 4097 'x' chars produces a JSON serialization well above 4096 bytes.
    let large_value = "x".repeat(4097);
    let body = serde_json::json!({
        "subject_id": subject_id,
        "attestation": { "data": large_value }
    })
    .to_string();

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
    assert!(
        json["error"].as_str().unwrap_or("").contains("attestation"),
        "error should mention attestation, got: {}",
        json["error"]
    );
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
async fn create_invite_rejects_oversized_attestation() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("inviteattest", db.pool()).await;

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy");
    // A string value of 4097 'x' chars produces a JSON serialization well above 4096 bytes.
    let large_value = "x".repeat(4097);
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": { "data": large_value }
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
        json["error"].as_str().unwrap_or("").contains("attestation"),
        "error should mention attestation, got: {}",
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
    assert!(
        json["error"].as_str().unwrap_or("").contains("fax"),
        "error should mention the invalid value, got: {}",
        json["error"]
    );
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
    assert!(
        json["error"].as_str().unwrap_or("").contains("decades"),
        "error should mention the invalid value, got: {}",
        json["error"]
    );
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

#[shared_runtime_test]
async fn denounce_rejects_whitespace_only_reason() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("denouncereason3", db.pool()).await;

    let (json2, _) = valid_signup_with_keys("denounceetarget3");
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
        "reason": "   "
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

/// When the endorser denounces the acceptor between invite creation and
/// acceptance, `accept_invite` returns 200 OK (the invite IS consumed) but the
/// auto-endorse step silently fails with `DenouncementConflict`.  This
/// documents the fire-and-forget contract so any future change that propagates
/// the error is caught by a test failure.
#[shared_runtime_test]
async fn accept_invite_succeeds_even_when_endorser_has_denounced_acceptor() {
    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, endorser_keys, endorser_id) =
        signup_and_get_account("conflictinviteendorser", db.pool()).await;

    // Sign up the acceptor.
    let (json2, acceptor_keys) = valid_signup_with_keys("conflictinviteacceptor");
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
    let body2 = axum::body::to_bytes(resp2.into_body(), 1024 * 1024)
        .await
        .expect("body2");
    let j2: Value = serde_json::from_slice(&body2).expect("json2");
    let acceptor_id: uuid::Uuid = j2["account_id"]
        .as_str()
        .expect("account_id")
        .parse()
        .expect("uuid");

    // Endorser creates an invite.
    let envelope_b64 = tc_crypto::encode_base64url(b"signed-invite-envelope");
    let invite_body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": {}
    })
    .to_string();
    let create_req = build_authed_request(
        Method::POST,
        "/trust/invites",
        &invite_body,
        &endorser_keys.device_signing_key,
        &endorser_keys.device_kid,
    );
    let create_resp = app.clone().oneshot(create_req).await.expect("create");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let invite_id = json_body(create_resp).await;
    let invite_id = invite_id["id"].as_str().expect("invite id");

    // Endorser denounces the acceptor directly via SQL (bypassing the API so
    // the daily action quota isn't consumed, which would fire QuotaExceeded
    // instead of DenouncementConflict).
    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(endorser_id)
    .bind(acceptor_id)
    .bind("changed my mind")
    .execute(&pool)
    .await
    .expect("seed denouncement");

    // Acceptor accepts the invite — 200 OK even though auto-endorse fails with DenouncementConflict.
    let accept_uri = format!("/trust/invites/{invite_id}/accept");
    let accept_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let accept_resp = app.clone().oneshot(accept_req).await.expect("accept");
    assert_eq!(accept_resp.status(), StatusCode::OK);

    // No endorsement action should have been queued — the denouncement blocks it.
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
        endorse_action.is_none(),
        "expected no endorse action when endorser has denounced acceptor, found: {pending:?}"
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
    use tinycongress_api::trust::service::ActionType;
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(account_id, ActionType::Endorse, &serde_json::json!({}))
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

/// Denouncing a user who was already denounced returns 409 Conflict.
/// This exercises the `AlreadyDenounced` service error path through the HTTP layer.
#[shared_runtime_test]
async fn denounce_returns_409_when_already_denounced() {
    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, keys, account_id) = signup_and_get_account("alreadydenouncer", db.pool()).await;

    // Sign up a target user.
    let (json2, _) = valid_signup_with_keys("alreadydenounced");
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

    // Seed an active denouncement directly (bypasses daily quota).
    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(account_id)
    .bind(target_id)
    .bind("initial reason")
    .execute(&pool)
    .await
    .expect("seed denouncement");

    // Attempt a second denouncement via HTTP — must return 409 Conflict.
    let body = serde_json::json!({
        "target_id": target_id,
        "reason": "trying again"
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
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

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
    use tinycongress_api::trust::service::ActionType;
    let trust_repo = PgTrustRepo::new(db.pool().clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(account_id, ActionType::Endorse, &serde_json::json!({}))
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

/// When the endorser has exhausted their daily action quota between invite
/// creation and acceptance, `accept_invite` returns 200 OK (the invite IS
/// consumed) but the auto-endorse step silently fails with `QuotaExceeded`.
/// This documents the fire-and-forget contract so any future change that
/// propagates the error is caught by a test failure.
#[shared_runtime_test]
async fn accept_invite_succeeds_even_when_endorser_quota_exceeded() {
    let db = isolated_db().await;
    let pool = db.pool().clone();
    let (app, endorser_keys, endorser_id) =
        signup_and_get_account("quotaexhaustedendorser", db.pool()).await;

    // Exhaust the endorser's daily action quota (5 actions) directly so the
    // invite-creation API call below doesn't count against it.
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    use tinycongress_api::trust::service::ActionType;
    let trust_repo = PgTrustRepo::new(pool.clone());
    for _ in 0..5 {
        trust_repo
            .enqueue_action(endorser_id, ActionType::Endorse, &serde_json::json!({}))
            .await
            .expect("enqueue quota filler");
    }

    // Endorser creates an invite (using the HTTP API; the invite-creation path
    // does not check the daily action quota, so this succeeds).
    let envelope_b64 = tc_crypto::encode_base64url(b"signed-invite-envelope");
    let invite_body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": {}
    })
    .to_string();
    let create_req = build_authed_request(
        Method::POST,
        "/trust/invites",
        &invite_body,
        &endorser_keys.device_signing_key,
        &endorser_keys.device_kid,
    );
    let create_resp = app.clone().oneshot(create_req).await.expect("create");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let invite_id = json_body(create_resp).await;
    let invite_id = invite_id["id"].as_str().expect("invite id");

    // Sign up the acceptor.
    let (json2, acceptor_keys) = valid_signup_with_keys("quotaexhaustedacceptor");
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

    // Acceptor accepts the invite — 200 OK even though auto-endorse fails with QuotaExceeded.
    let accept_uri = format!("/trust/invites/{invite_id}/accept");
    let accept_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let accept_resp = app.clone().oneshot(accept_req).await.expect("accept");
    assert_eq!(accept_resp.status(), StatusCode::OK);

    // No additional endorse action should have been queued — the quota check
    // prevents it.  The only actions in the log are the 5 quota-filler rows.
    let pending = sqlx::query_as::<_, tinycongress_api::trust::repo::ActionRecord>(
        "SELECT * FROM trust__action_log WHERE status = 'pending' ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .expect("query pending actions");
    let real_endorse_action = pending.iter().find(|a| {
        a.actor_id == endorser_id
            && a.action_type == "endorse"
            && a.payload != serde_json::json!({})
    });
    assert!(
        real_endorse_action.is_none(),
        "expected no real endorse action when endorser quota is exhausted, found: {pending:?}"
    );

    let _ = (endorser_keys, acceptor_keys);
}

/// A second attempt to accept an already-accepted invite returns 404.
///
/// The `accept_invite` SQL atomically marks an invite as taken by requiring
/// `accepted_by IS NULL`.  Once a first acceptor claims the invite, any
/// subsequent `accept_invite` call — even from the same user — finds no
/// matching row and returns `TrustRepoError::NotFound`, which the handler
/// maps to a 404 response.  This is distinct from the "invite UUID does not
/// exist" 404 tested elsewhere.
#[shared_runtime_test]
async fn test_accept_already_accepted_invite_returns_404() {
    let db = isolated_db().await;

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
    let envelope_b64 = tc_crypto::encode_base64url(b"already-accepted-envelope");
    let invite_body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "qr",
        "attestation": {}
    })
    .to_string();
    let create_req = build_authed_request(
        Method::POST,
        "/trust/invites",
        &invite_body,
        &endorser_keys.device_signing_key,
        &endorser_keys.device_kid,
    );
    let create_resp = app.clone().oneshot(create_req).await.expect("create");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let invite_id = json_body(create_resp).await;
    let invite_id = invite_id["id"].as_str().expect("invite id").to_string();

    let accept_uri = format!("/trust/invites/{invite_id}/accept");

    // First acceptance: succeeds.
    let first_req = build_authed_request(
        Method::POST,
        &accept_uri,
        "",
        &acceptor_keys.device_signing_key,
        &acceptor_keys.device_kid,
    );
    let first_resp = app.clone().oneshot(first_req).await.expect("first accept");
    assert_eq!(
        first_resp.status(),
        StatusCode::OK,
        "first acceptance must succeed"
    );

    // Second acceptance of the same invite: `accepted_by IS NULL` is no longer
    // satisfied, so `accept_invite` returns NotFound → handler returns 404.
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
    assert_eq!(
        second_resp.status(),
        StatusCode::NOT_FOUND,
        "second attempt to accept an already-accepted invite must return 404"
    );

    let _ = (endorser_keys, acceptor_keys);
}

// ─── Accept invite — data integrity guard ─────────────────────────────────────

/// When `accept_invite` succeeds but returns an `InviteRecord` with
/// `accepted_at = None`, the handler returns 500 Internal Server Error.
///
/// This invariant cannot occur with the real PostgreSQL implementation because
/// the UPDATE always sets `accepted_at = now()`.  A stub repo simulates the
/// impossible-but-defensive case to confirm the guard fires correctly.
#[shared_runtime_test]
async fn accept_invite_returns_500_when_accepted_at_is_none() {
    let db = isolated_db().await;
    // Sign up an acceptor so we have a valid authenticated device to make the request.
    let (_, keys, account_id) = signup_and_get_account("acceptorinvariantcheck", db.pool()).await;

    // The endorser must be a different user so the self-accept guard doesn't
    // fire before we reach the `accepted_at` check.
    let endorser_id = Uuid::new_v4();
    assert_ne!(
        endorser_id, account_id,
        "stub endorser_id must differ from acceptor"
    );

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubAcceptInviteNullTimestamp { endorser_id }))
        .with_stub_trust_service(Arc::new(PanickingTrustService))
        .build();

    let invite_id = Uuid::new_v4();
    let uri = format!("/trust/invites/{invite_id}/accept");
    let request = build_authed_request(
        Method::POST,
        &uri,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "handler must return 500 when accept_invite returns an InviteRecord with accepted_at = None"
    );
}

// ─── Stub ReputationRepo for budget 500 error ────────────────────────────────

/// Stub [`ReputationRepo`] that returns an error from
/// `count_active_trust_endorsements_by`, simulating the first early-return 500
/// path in `budget_handler`.  All other methods panic — they must never be
/// reached in this test.
struct StubBudgetReputationRepoReturnsError;

#[async_trait]
impl ReputationRepo for StubBudgetReputationRepoReturnsError {
    async fn count_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Err(EndorsementRepoError::NotFound)
    }

    async fn count_all_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }

    async fn create_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
        _endorser_id: Option<Uuid>,
        _evidence: Option<&serde_json::Value>,
        _weight: f32,
        _attestation: Option<&serde_json::Value>,
        _in_slot: bool,
    ) -> Result<CreatedEndorsement, EndorsementRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }

    async fn has_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<bool, EndorsementRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }

    async fn list_endorsements_by_subject(
        &self,
        _subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }

    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<(), EndorsementRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }

    async fn link_external_identity(
        &self,
        _account_id: Uuid,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }

    async fn get_external_identity_by_provider(
        &self,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetReputationRepoReturnsError: not needed for this test")
    }
}

/// Stub [`TrustRepo`] that panics on every call.
///
/// `budget_handler` returns early with 500 before reaching any `TrustRepo`
/// call when the reputation repo fails, so this stub must never be invoked
/// in the test below.
struct PanickingTrustRepo;

#[async_trait]
impl TrustRepo for PanickingTrustRepo {
    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("PanickingTrustRepo: must not be called in this test")
    }
}

/// When `count_active_trust_endorsements_by` returns an error, `budget_handler`
/// must return 500 Internal Server Error before reaching the `TrustRepo` call.
///
/// A stub repo simulates the database failure to confirm the guard fires correctly.
#[shared_runtime_test]
async fn budget_returns_500_when_endorsement_count_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("budgeterror", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(PanickingTrustRepo))
        .with_stub_reputation_repo(Arc::new(StubBudgetReputationRepoReturnsError))
        .build();

    let request = build_authed_request(
        Method::GET,
        "/trust/budget",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "budget_handler must return 500 when endorsement count query fails"
    );
}

// ─── Stub ReputationRepo for budget all-endorsements 500 error ───────────────

/// Stub [`ReputationRepo`] that lets `count_active_trust_endorsements_by` succeed
/// but returns an error from `count_all_active_trust_endorsements_by`, simulating
/// the second early-return 500 path in `budget_handler`.  All other methods panic.
struct StubBudgetAllEndorsementsReturnsError;

#[async_trait]
impl ReputationRepo for StubBudgetAllEndorsementsReturnsError {
    async fn count_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Ok(0)
    }

    async fn count_all_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Err(EndorsementRepoError::NotFound)
    }

    async fn create_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
        _endorser_id: Option<Uuid>,
        _evidence: Option<&serde_json::Value>,
        _weight: f32,
        _attestation: Option<&serde_json::Value>,
        _in_slot: bool,
    ) -> Result<CreatedEndorsement, EndorsementRepoError> {
        unimplemented!("StubBudgetAllEndorsementsReturnsError: not needed for this test")
    }

    async fn has_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<bool, EndorsementRepoError> {
        unimplemented!("StubBudgetAllEndorsementsReturnsError: not needed for this test")
    }

    async fn list_endorsements_by_subject(
        &self,
        _subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
        unimplemented!("StubBudgetAllEndorsementsReturnsError: not needed for this test")
    }

    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<(), EndorsementRepoError> {
        unimplemented!("StubBudgetAllEndorsementsReturnsError: not needed for this test")
    }

    async fn link_external_identity(
        &self,
        _account_id: Uuid,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetAllEndorsementsReturnsError: not needed for this test")
    }

    async fn get_external_identity_by_provider(
        &self,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetAllEndorsementsReturnsError: not needed for this test")
    }
}

/// When `count_all_active_trust_endorsements_by` returns an error,
/// `budget_handler` must return 500 Internal Server Error before reaching the
/// `TrustRepo` call.
///
/// The first reputation-repo call succeeds (returns 0); the second fails.
#[shared_runtime_test]
async fn budget_returns_500_when_all_endorsements_count_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("budgetallerror", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(PanickingTrustRepo))
        .with_stub_reputation_repo(Arc::new(StubBudgetAllEndorsementsReturnsError))
        .build();

    let request = build_authed_request(
        Method::GET,
        "/trust/budget",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "budget_handler must return 500 when all-endorsements count query fails"
    );
}

// ─── Stub ReputationRepo where both endorsement counts succeed ────────────────

/// Stub [`ReputationRepo`] that lets both `count_active_trust_endorsements_by`
/// and `count_all_active_trust_endorsements_by` succeed, allowing
/// `budget_handler` to proceed to the `TrustRepo` call.  All other methods
/// panic — they must never be reached in this test.
struct StubBudgetBothEndorsementsSucceed;

#[async_trait]
impl ReputationRepo for StubBudgetBothEndorsementsSucceed {
    async fn count_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Ok(0)
    }

    async fn count_all_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Ok(0)
    }

    async fn create_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
        _endorser_id: Option<Uuid>,
        _evidence: Option<&serde_json::Value>,
        _weight: f32,
        _attestation: Option<&serde_json::Value>,
        _in_slot: bool,
    ) -> Result<CreatedEndorsement, EndorsementRepoError> {
        unimplemented!("StubBudgetBothEndorsementsSucceed: not needed for this test")
    }

    async fn has_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<bool, EndorsementRepoError> {
        unimplemented!("StubBudgetBothEndorsementsSucceed: not needed for this test")
    }

    async fn list_endorsements_by_subject(
        &self,
        _subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
        unimplemented!("StubBudgetBothEndorsementsSucceed: not needed for this test")
    }

    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<(), EndorsementRepoError> {
        unimplemented!("StubBudgetBothEndorsementsSucceed: not needed for this test")
    }

    async fn link_external_identity(
        &self,
        _account_id: Uuid,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetBothEndorsementsSucceed: not needed for this test")
    }

    async fn get_external_identity_by_provider(
        &self,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetBothEndorsementsSucceed: not needed for this test")
    }
}

// ─── Stub TrustRepo for budget denouncements 500 error ───────────────────────

/// Stub [`TrustRepo`] that returns an error from `count_total_denouncements_by`,
/// simulating the third early-return 500 path in `budget_handler`.  All other
/// methods panic — they must never be reached in this test.
struct StubBudgetTrustRepoDenouncementsError;

#[async_trait]
impl TrustRepo for StubBudgetTrustRepoDenouncementsError {
    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        Err(TrustRepoError::NotFound)
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubBudgetTrustRepoDenouncementsError: must not be called in this test")
    }
}

/// When `count_total_denouncements_by` returns an error, `budget_handler`
/// must return 500 Internal Server Error.
///
/// Both endorsement-count calls succeed (returns 0); the failure occurs at the
/// `TrustRepo` step. This tests the third early-return 500 path in
/// `budget_handler`, complementing the two endorsement-count error tests above.
#[shared_runtime_test]
async fn budget_returns_500_when_denouncements_count_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("budgetdenounceerr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubBudgetTrustRepoDenouncementsError))
        .with_stub_reputation_repo(Arc::new(StubBudgetBothEndorsementsSucceed))
        .build();

    let request = build_authed_request(
        Method::GET,
        "/trust/budget",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "budget_handler must return 500 when denouncement count query fails"
    );
}

// ─── Stub TrustRepo for scores_me 500 error ──────────────────────────────────

/// Stub [`TrustRepo`] that returns a database error from `get_all_scores`,
/// simulating a failing query in `scores_me_handler`.  All other methods
/// panic — they must never be reached in this test.
struct StubScoresMeReturnsError;

#[async_trait]
impl TrustRepo for StubScoresMeReturnsError {
    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubScoresMeReturnsError: must not be called in this test")
    }
}

// ─── Stub TrustRepo for list_my_denouncements 500 error ─────────────────────

/// Stub [`TrustRepo`] that returns a database error from
/// `list_denouncements_by_with_username`, simulating a failing query in
/// `list_my_denouncements_handler`.  All other methods panic — they must never
/// be reached in this test.
struct StubListDenouncementsReturnsError;

#[async_trait]
impl TrustRepo for StubListDenouncementsReturnsError {
    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubListDenouncementsReturnsError: must not be called in this test")
    }
}

/// When `list_denouncements_by_with_username` returns a database error,
/// `list_my_denouncements_handler` must return 500 Internal Server Error.
///
/// A stub repo simulates the database failure; the handler must propagate it
/// via `trust_repo_error_response` rather than panic or swallow it.
#[shared_runtime_test]
async fn list_my_denouncements_returns_500_when_db_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("denouncementdberr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubListDenouncementsReturnsError))
        .build();

    let request = build_authed_request(
        Method::GET,
        "/trust/denouncements/mine",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "list_my_denouncements_handler must return 500 when list_denouncements_by_with_username query fails"
    );
}

/// When `get_all_scores` returns a database error, `scores_me_handler` must
/// return 500 Internal Server Error.
///
/// A stub repo simulates the database failure; the handler must propagate it
/// via `trust_repo_error_response` rather than panic or swallow it.
#[shared_runtime_test]
async fn scores_me_returns_500_when_get_all_scores_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("scoresmeerr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubScoresMeReturnsError))
        .build();

    let request = build_authed_request(
        Method::GET,
        "/trust/scores/me",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "scores_me_handler must return 500 when get_all_scores query fails"
    );
}

// ─── Stub TrustRepo for list_invites_handler 500 error ───────────────────────

/// Stub [`TrustRepo`] that returns a database error from
/// `list_invites_by_endorser`, simulating a failing query in
/// `list_invites_handler`.  All other methods panic — they must never be
/// reached in this test.
struct StubListInvitesReturnsError;

#[async_trait]
impl TrustRepo for StubListInvitesReturnsError {
    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubListInvitesReturnsError: must not be called in this test")
    }
}

/// When `list_invites_by_endorser` returns a database error,
/// `list_invites_handler` must return 500 Internal Server Error.
///
/// A stub repo simulates the database failure; the handler must propagate it
/// via `trust_repo_error_response` rather than panic or swallow it.
#[shared_runtime_test]
async fn list_invites_returns_500_when_db_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("listinvitesdberr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubListInvitesReturnsError))
        .build();

    let request = build_authed_request(
        Method::GET,
        "/trust/invites/mine",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "list_invites_handler must return 500 when list_invites_by_endorser query fails"
    );
}

// ─── Stub TrustRepo for create_invite_handler 500 error ──────────────────────

/// Stub [`TrustRepo`] that returns a database error from `create_invite`,
/// simulating a failing insert in `create_invite_handler`.  All other methods
/// panic — they must never be reached in this test.
struct StubCreateInviteReturnsError;

#[async_trait]
impl TrustRepo for StubCreateInviteReturnsError {
    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubCreateInviteReturnsError: must not be called in this test")
    }
}

/// When `create_invite` returns a database error, `create_invite_handler` must
/// return 500 Internal Server Error.
///
/// A stub repo simulates the database failure; the handler must propagate it
/// via `trust_repo_error_response` rather than panic or swallow it.
#[shared_runtime_test]
async fn create_invite_handler_returns_500_when_db_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("createinvitedberr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubCreateInviteReturnsError))
        .build();

    let envelope_b64 = tc_crypto::encode_base64url(b"dummy-envelope");
    let body = serde_json::json!({
        "envelope": envelope_b64,
        "delivery_method": "email",
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
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "create_invite_handler must return 500 when create_invite query fails"
    );
}

// ─── Stub TrustRepo for accept_invite_handler accept_invite 500 error ────────

/// Stub [`TrustRepo`] that returns Ok from `get_invite` (so the self-accept
/// guard passes) and a database error from `accept_invite`, simulating a
/// failing insert in `accept_invite_handler`.  All other methods panic.
struct StubAcceptInviteAcceptReturnsError {
    endorser_id: Uuid,
}

#[async_trait]
impl TrustRepo for StubAcceptInviteAcceptReturnsError {
    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        Ok(InviteRecord {
            id: Uuid::new_v4(),
            endorser_id: self.endorser_id,
            envelope: vec![0u8],
            delivery_method: "qr".to_string(),
            attestation: serde_json::Value::Object(serde_json::Map::new()),
            accepted_by: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(7),
            accepted_at: None,
            created_at: chrono::Utc::now(),
            relationship_depth: None,
            weight: 1.0,
        })
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        Err(TrustRepoError::Database(sqlx::Error::RowNotFound))
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubAcceptInviteAcceptReturnsError: must not be called in this test")
    }
}

/// When `accept_invite` returns a database error, `accept_invite_handler` must
/// return 500 Internal Server Error.
///
/// The stub returns Ok from `get_invite` (with a different endorser so the
/// self-accept guard passes), then returns a database error from `accept_invite`.
/// The handler must propagate it via `trust_repo_error_response`.
#[shared_runtime_test]
async fn accept_invite_handler_returns_500_when_accept_invite_db_fails() {
    let db = isolated_db().await;
    let (_, keys, account_id) = signup_and_get_account("acceptinvitedberr", db.pool()).await;

    // endorser_id must differ from account_id so the self-accept guard does not fire.
    let endorser_id = Uuid::new_v4();
    assert_ne!(
        endorser_id, account_id,
        "stub endorser_id must differ from acceptor"
    );

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubAcceptInviteAcceptReturnsError { endorser_id }))
        .with_stub_trust_service(Arc::new(PanickingTrustService))
        .build();

    let invite_id = Uuid::new_v4();
    let uri = format!("/trust/invites/{invite_id}/accept");
    let request = build_authed_request(
        Method::POST,
        &uri,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "accept_invite_handler must return 500 when accept_invite query fails"
    );
}

// ─── Stub TrustService for endorse_handler 500 error ─────────────────────────

/// Stub [`TrustService`] that returns a database error from `endorse`,
/// simulating a repo failure propagated as [`TrustServiceError::Repo`].
/// All other methods panic — they must never be reached in this test.
struct StubEndorseServiceDbError;

#[async_trait]
impl TrustService for StubEndorseServiceDbError {
    async fn endorse(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _weight: f32,
        _attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError> {
        Err(TrustServiceError::Repo(TrustRepoError::Database(
            sqlx::Error::RowNotFound,
        )))
    }
    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubEndorseServiceDbError: must not be called in this test")
    }
    async fn denounce(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubEndorseServiceDbError: must not be called in this test")
    }
}

/// Stub [`TrustRepo`] that panics on every method.
///
/// Used in tests that exercise a handler code path that calls only the
/// [`TrustService`] extension, never the [`TrustRepo`] extension directly.
/// Registered solely to satisfy the `include_trust` requirement in
/// [`TestAppBuilder`].
struct NeverCalledTrustRepo;

#[async_trait]
impl TrustRepo for NeverCalledTrustRepo {
    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("NeverCalledTrustRepo: must not be called in this test")
    }
}

// ─── Stub TrustService for revoke_handler 500 error ──────────────────────────

/// Stub [`TrustService`] that returns a database error from `revoke_endorsement`,
/// simulating a repo failure propagated as [`TrustServiceError::Repo`].
/// All other methods panic — they must never be reached in this test.
struct StubRevokeServiceDbError;

#[async_trait]
impl TrustService for StubRevokeServiceDbError {
    async fn endorse(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _weight: f32,
        _attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubRevokeServiceDbError: must not be called in this test")
    }
    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        Err(TrustServiceError::Repo(TrustRepoError::Database(
            sqlx::Error::RowNotFound,
        )))
    }
    async fn denounce(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubRevokeServiceDbError: must not be called in this test")
    }
}

/// When the trust service returns a database error, `revoke_handler` must
/// return 500 Internal Server Error.
///
/// `TrustServiceError::Repo` wraps the underlying repo failure and maps to
/// 500 via `trust_service_error_response`.  This covers the code path where
/// an unexpected DB error surfaces through the service layer rather than a
/// user-visible error like `QuotaExceeded` or `SelfAction`.
#[shared_runtime_test]
async fn revoke_handler_returns_500_when_service_db_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("revokesvcerr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(NeverCalledTrustRepo))
        .with_stub_trust_service(Arc::new(StubRevokeServiceDbError))
        .build();

    let body = serde_json::json!({
        "subject_id": Uuid::new_v4(),
    })
    .to_string();
    let request = build_authed_request(
        Method::POST,
        "/trust/revoke",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "revoke_handler must return 500 when the service propagates a database error"
    );
}

// ─── Stub TrustService for denounce_handler 500 error ────────────────────────

/// Stub [`TrustService`] that returns a database error from `denounce`,
/// simulating a repo failure propagated as [`TrustServiceError::Repo`].
/// All other methods panic — they must never be reached in this test.
struct StubDenounceServiceDbError;

#[async_trait]
impl TrustService for StubDenounceServiceDbError {
    async fn endorse(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _weight: f32,
        _attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubDenounceServiceDbError: must not be called in this test")
    }
    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubDenounceServiceDbError: must not be called in this test")
    }
    async fn denounce(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<(), TrustServiceError> {
        Err(TrustServiceError::Repo(TrustRepoError::Database(
            sqlx::Error::RowNotFound,
        )))
    }
}

/// When the trust service returns a database error, `denounce_handler` must
/// return 500 Internal Server Error.
///
/// `TrustServiceError::Repo` wraps the underlying repo failure and maps to
/// 500 via `trust_service_error_response`.  This covers the code path where
/// an unexpected DB error surfaces through the service layer rather than a
/// user-visible error like `QuotaExceeded` or `SelfAction`.
#[shared_runtime_test]
async fn denounce_handler_returns_500_when_service_db_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("denouncesvcerr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(NeverCalledTrustRepo))
        .with_stub_trust_service(Arc::new(StubDenounceServiceDbError))
        .build();

    let body = serde_json::json!({
        "target_id": Uuid::new_v4(),
        "reason": "test reason"
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
    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "denounce_handler must return 500 when the service propagates a database error"
    );
}

// ─── Stub TrustService for endorse_handler 500 error ─────────────────────────

// ─── Stub TrustService for endorse_handler EndorsementRepo 500 error ─────────

/// Stub [`TrustService`] that returns an endorsement repo error from `endorse`,
/// simulating a failure propagated as [`TrustServiceError::EndorsementRepo`].
/// All other methods panic — they must never be reached in this test.
struct StubEndorseServiceEndorsementRepoError;

#[async_trait]
impl TrustService for StubEndorseServiceEndorsementRepoError {
    async fn endorse(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _weight: f32,
        _attestation: Option<serde_json::Value>,
    ) -> Result<(), TrustServiceError> {
        Err(TrustServiceError::EndorsementRepo(
            EndorsementRepoError::Database(sqlx::Error::RowNotFound),
        ))
    }
    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubEndorseServiceEndorsementRepoError: must not be called in this test")
    }
    async fn denounce(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<(), TrustServiceError> {
        unimplemented!("StubEndorseServiceEndorsementRepoError: must not be called in this test")
    }
}

/// When the trust service returns an endorsement repo error, `endorse_handler`
/// must return 500 Internal Server Error.
///
/// `TrustServiceError::EndorsementRepo` wraps a reputation repo failure and maps
/// to 500 via `trust_service_error_response`. This exercises the distinct match
/// arm for `EndorsementRepo` (as opposed to `Repo`, which wraps `TrustRepoError`).
/// The `EndorsementRepo` variant is reachable in production when the verifier
/// check (`has_endorsement`) or slot count (`count_active_trust_endorsements_by`)
/// fails during `DefaultTrustService::endorse`.
#[shared_runtime_test]
async fn endorse_handler_returns_500_when_service_propagates_endorsement_repo_error() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("endorserepofail", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(NeverCalledTrustRepo))
        .with_stub_trust_service(Arc::new(StubEndorseServiceEndorsementRepoError))
        .build();

    let body = serde_json::json!({
        "subject_id": Uuid::new_v4(),
        "weight": 1.0
    })
    .to_string();
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
        StatusCode::INTERNAL_SERVER_ERROR,
        "endorse_handler must return 500 when service propagates EndorsementRepo error"
    );
}

// ─── Stub TrustRepo for accept_invite auto-endorse DB error ──────────────────

/// Stub [`TrustRepo`] that returns a valid invite from `get_invite` (with a
/// different `endorser_id` so the self-accept guard passes) and a valid
/// accepted invite from `accept_invite` (with `accepted_at` set).
/// All other methods panic — this stub is only valid for the
/// `accept_invite_handler` fire-and-forget auto-endorse code path.
struct StubAcceptInviteSuccessRepo {
    endorser_id: Uuid,
}

impl StubAcceptInviteSuccessRepo {
    fn invite_record(&self, accepted_at: Option<chrono::DateTime<chrono::Utc>>) -> InviteRecord {
        InviteRecord {
            id: Uuid::new_v4(),
            endorser_id: self.endorser_id,
            envelope: vec![0u8],
            delivery_method: "qr".to_string(),
            attestation: serde_json::Value::Object(serde_json::Map::new()),
            accepted_by: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(7),
            accepted_at,
            created_at: chrono::Utc::now(),
            relationship_depth: None,
            weight: 1.0,
        }
    }
}

#[async_trait]
impl TrustRepo for StubAcceptInviteSuccessRepo {
    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        Ok(self.invite_record(None))
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        Ok(self.invite_record(Some(chrono::Utc::now())))
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!("StubAcceptInviteSuccessRepo: must not be called in this test")
    }
}

/// When the auto-endorse step in `accept_invite_handler` fails with a database
/// error, the handler must still return 200 OK.
///
/// The auto-endorse is fire-and-forget: a warning is logged and the invite
/// acceptance itself (already committed) is returned to the caller.  This test
/// ensures that a repo-level failure in `TrustService::endorse` does not
/// propagate back to the HTTP response — a regression guard for the
/// `if let Err(e) = trust_service.endorse(...)` branch.
#[shared_runtime_test]
async fn accept_invite_handler_returns_200_when_auto_endorse_fails_with_db_error() {
    let db = isolated_db().await;
    let (_, keys, account_id) = signup_and_get_account("acceptinviteautoenderr", db.pool()).await;

    // endorser_id must differ from account_id so the self-accept guard does not fire.
    let endorser_id = Uuid::new_v4();
    assert_ne!(
        endorser_id, account_id,
        "stub endorser_id must differ from acceptor"
    );

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubAcceptInviteSuccessRepo { endorser_id }))
        .with_stub_trust_service(Arc::new(StubEndorseServiceDbError))
        .build();

    let invite_id = Uuid::new_v4();
    let uri = format!("/trust/invites/{invite_id}/accept");
    let request = build_authed_request(
        Method::POST,
        &uri,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "accept_invite_handler must return 200 even when auto-endorse fails with a db error"
    );
}

/// When the trust service returns a database error, `endorse_handler` must
/// return 500 Internal Server Error.
///
/// `TrustServiceError::Repo` wraps the underlying repo failure and maps to
/// 500 via `trust_service_error_response`.  This covers the code path where
/// an unexpected DB error surfaces through the service layer rather than a
/// user-visible error like `QuotaExceeded` or `SelfAction`.
#[shared_runtime_test]
async fn endorse_handler_returns_500_when_service_db_fails() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("endorsesvcerr", db.pool()).await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(NeverCalledTrustRepo))
        .with_stub_trust_service(Arc::new(StubEndorseServiceDbError))
        .build();

    let body = serde_json::json!({
        "subject_id": Uuid::new_v4(),
        "weight": 1.0
    })
    .to_string();
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
        StatusCode::INTERNAL_SERVER_ERROR,
        "endorse_handler must return 500 when the service propagates a database error"
    );
}

// ─── Stub ReputationRepo simulating concurrent revocation ─────────────────────

/// Stub [`ReputationRepo`] that returns a higher in-slot count than total-endorsement
/// count, simulating a concurrent revocation between the two queries.
///
/// `count_active_trust_endorsements_by` returns 3 (in-slot only).
/// `count_all_active_trust_endorsements_by` returns 2 (all active — fewer because a
/// concurrent revocation completed between the two queries).
///
/// This produces a negative raw difference (-1) that `budget_handler` must clamp to 0.
struct StubBudgetRepoConcurrentRevocation;

#[async_trait]
impl ReputationRepo for StubBudgetRepoConcurrentRevocation {
    async fn count_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Ok(3)
    }

    async fn count_all_active_trust_endorsements_by(
        &self,
        _endorser_id: Uuid,
    ) -> Result<i64, EndorsementRepoError> {
        Ok(2)
    }

    async fn create_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
        _endorser_id: Option<Uuid>,
        _evidence: Option<&serde_json::Value>,
        _weight: f32,
        _attestation: Option<&serde_json::Value>,
        _in_slot: bool,
    ) -> Result<CreatedEndorsement, EndorsementRepoError> {
        unimplemented!("StubBudgetRepoConcurrentRevocation: not needed for this test")
    }

    async fn has_endorsement(
        &self,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<bool, EndorsementRepoError> {
        unimplemented!("StubBudgetRepoConcurrentRevocation: not needed for this test")
    }

    async fn list_endorsements_by_subject(
        &self,
        _subject_id: Uuid,
    ) -> Result<Vec<EndorsementRecord>, EndorsementRepoError> {
        unimplemented!("StubBudgetRepoConcurrentRevocation: not needed for this test")
    }

    async fn revoke_endorsement(
        &self,
        _endorser_id: Uuid,
        _subject_id: Uuid,
        _topic: &str,
    ) -> Result<(), EndorsementRepoError> {
        unimplemented!("StubBudgetRepoConcurrentRevocation: not needed for this test")
    }

    async fn link_external_identity(
        &self,
        _account_id: Uuid,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetRepoConcurrentRevocation: not needed for this test")
    }

    async fn get_external_identity_by_provider(
        &self,
        _provider: &str,
        _provider_subject: &str,
    ) -> Result<ExternalIdentityRecord, ExternalIdentityRepoError> {
        unimplemented!("StubBudgetRepoConcurrentRevocation: not needed for this test")
    }
}

// ─── Stub TrustRepo returning zero denouncements ──────────────────────────────

/// Stub [`TrustRepo`] that returns `Ok(0)` from `count_total_denouncements_by`.
/// All other methods panic — they must never be reached in this test.
struct StubBudgetTrustRepoZeroDenouncementsSucceed;

#[async_trait]
impl TrustRepo for StubBudgetTrustRepoZeroDenouncementsSucceed {
    async fn count_total_denouncements_by(&self, _accuser_id: Uuid) -> Result<i64, TrustRepoError> {
        Ok(0)
    }

    async fn get_or_create_influence(
        &self,
        _user_id: Uuid,
    ) -> Result<InfluenceRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn enqueue_action(
        &self,
        _actor_id: Uuid,
        _action_type: ActionType,
        _payload: &serde_json::Value,
    ) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn count_daily_actions(&self, _actor_id: Uuid) -> Result<i64, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn get_action(&self, _action_id: Uuid) -> Result<ActionRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn complete_action(&self, _action_id: Uuid) -> Result<(), TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn fail_action(&self, _action_id: Uuid, _error: &str) -> Result<(), TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn create_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn create_denouncement_and_revoke_endorsement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
        _reason: &str,
    ) -> Result<DenouncementRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn list_denouncements_against(
        &self,
        _target_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn list_denouncements_by(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementRecord>, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn list_denouncements_by_with_username(
        &self,
        _accuser_id: Uuid,
    ) -> Result<Vec<DenouncementWithUsername>, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn has_active_denouncement(
        &self,
        _accuser_id: Uuid,
        _target_id: Uuid,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn create_invite(
        &self,
        _endorser_id: Uuid,
        _envelope: &[u8],
        _delivery_method: DeliveryMethod,
        _relationship_depth: Option<RelationshipDepth>,
        _weight: f32,
        _attestation: &serde_json::Value,
        _expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn get_invite(&self, _invite_id: Uuid) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn accept_invite(
        &self,
        _invite_id: Uuid,
        _accepted_by: Uuid,
    ) -> Result<InviteRecord, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn list_invites_by_endorser(
        &self,
        _endorser_id: Uuid,
    ) -> Result<Vec<InviteRecord>, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn upsert_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
        _distance: Option<f32>,
        _diversity: Option<i32>,
        _centrality: Option<f32>,
    ) -> Result<(), TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn get_score(
        &self,
        _user_id: Uuid,
        _context_user_id: Option<Uuid>,
    ) -> Result<Option<ScoreSnapshot>, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn get_all_scores(&self, _user_id: Uuid) -> Result<Vec<ScoreSnapshot>, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }

    async fn has_identity_endorsement(
        &self,
        _user_id: Uuid,
        _verifier_ids: &[Uuid],
        _topic: &str,
    ) -> Result<bool, TrustRepoError> {
        unimplemented!(
            "StubBudgetTrustRepoZeroDenouncementsSucceed: must not be called in this test"
        )
    }
}

/// When a concurrent revocation completes between the two endorsement-count queries,
/// `count_all_active_trust_endorsements_by` can return a value smaller than
/// `count_active_trust_endorsements_by`, making the raw difference negative.
/// `budget_handler` must clamp this to zero rather than returning a negative
/// `out_of_slot_count` to the client.
#[shared_runtime_test]
async fn budget_clamps_out_of_slot_count_to_zero_on_concurrent_revocation() {
    let db = isolated_db().await;
    let (_, keys, _) = signup_and_get_account("budgetclamp", db.pool()).await;

    // in-slot count (3) > all-endorsements count (2): simulates a revocation that
    // completed between the two queries, making the raw difference -1.
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .with_stub_trust_repo(Arc::new(StubBudgetTrustRepoZeroDenouncementsSucceed))
        .with_stub_reputation_repo(Arc::new(StubBudgetRepoConcurrentRevocation))
        .build();

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
    assert_eq!(
        json["out_of_slot_count"], 0,
        "out_of_slot_count must be clamped to 0 when all_endorsements < endorsements_used"
    );
}
