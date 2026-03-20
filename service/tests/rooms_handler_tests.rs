//! Integration tests for rooms and polling endpoints.
//!
//! Tests cover the full stack: HTTP → service → repo, including
//! eligibility checks via the endorsement system.

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
    let app = TestAppBuilder::new().with_rooms_pool(pool.clone()).build();

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

/// Helper: create a genesis endorsement for a user (used for reputation endpoint tests).
async fn endorse_user(pool: &sqlx::PgPool, account_id: uuid::Uuid, topic: &str) {
    use tinycongress_api::reputation::repo::create_endorsement;

    create_endorsement(pool, account_id, topic, None, None, 1.0, None, true)
        .await
        .expect("endorsement");
}

/// Helper: create or return a deterministic anchor account for constraint tests.
///
/// Uses seed 200 so it never collides with test user seeds. Returns the anchor's UUID.
/// Safe to call multiple times on the same DB — returns the existing account on conflict.
async fn get_or_create_anchor(pool: &sqlx::PgPool) -> uuid::Uuid {
    use common::factories::{generate_test_keys, AccountFactory};
    use tinycongress_api::identity::repo::AccountRepoError;

    match AccountFactory::new().with_seed(200).create(pool).await {
        Ok(account) => account.id,
        Err(AccountRepoError::DuplicateKey | AccountRepoError::DuplicateUsername) => {
            let (_, root_kid) = generate_test_keys(200);
            sqlx::query_scalar("SELECT id FROM accounts WHERE root_kid = $1")
                .bind(root_kid.as_str())
                .fetch_one(pool)
                .await
                .expect("find existing anchor account")
        }
        Err(e) => panic!("create anchor account: {e}"),
    }
}

/// Helper: configure a room to use `identity_verified` constraint with the test verifier.
///
/// Sets constraint_type = 'identity_verified' and constraint_config = {"verifier_ids": [verifier_id]}.
/// Rooms created via POST /rooms already default to identity_verified; this helper ensures
/// the verifier_ids config is set so `build_constraint` succeeds.
async fn set_room_anchor(pool: &sqlx::PgPool, room_id: uuid::Uuid) {
    let verifier = get_or_create_anchor(pool).await;
    sqlx::query(
        "UPDATE rooms__rooms SET constraint_type = 'identity_verified', constraint_config = $1 WHERE id = $2",
    )
    .bind(serde_json::json!({"verifier_ids": [verifier]}))
    .bind(room_id)
    .execute(pool)
    .await
    .expect("set room constraint");
}

/// Helper: make `account_id` eligible to vote in a room using `identity_verified` constraint.
///
/// Configures the room's constraint to use a test verifier, then creates an
/// `identity_verified` endorsement from that verifier to the account. The
/// `IdentityVerifiedConstraint` will find the endorsement and mark the user eligible.
async fn make_eligible(pool: &sqlx::PgPool, account_id: uuid::Uuid, room_id: uuid::Uuid) {
    let verifier = get_or_create_anchor(pool).await;

    set_room_anchor(pool, room_id).await;

    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
         VALUES ($1, $2, 'identity_verified', 1.0) ON CONFLICT DO NOTHING",
    )
    .bind(verifier)
    .bind(account_id)
    .execute(pool)
    .await
    .expect("identity endorsement");
}

/// Helper: parse JSON response body.
async fn json_body(response: axum::http::Response<Body>) -> Value {
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    serde_json::from_slice(&body).expect("json")
}

// ─── Room CRUD ───────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_list_rooms_empty() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/rooms")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body.as_array().expect("array").len(), 0);
}

#[shared_runtime_test]
async fn test_create_room_authenticated() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("roomcreator", db.pool()).await;

    let body = serde_json::json!({
        "name": "Climate Room",
        "description": "Discuss climate policy",
        "eligibility_topic": "identity_verified"
    })
    .to_string();

    let request = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = json_body(response).await;
    assert_eq!(json["name"], "Climate Room");
    assert_eq!(json["eligibility_topic"], "identity_verified");
    assert_eq!(json["status"], "open");
    assert!(json["id"].is_string());
}

#[shared_runtime_test]
async fn test_create_room_unauthenticated_returns_401() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/rooms")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Test"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[shared_runtime_test]
async fn test_get_room_by_id() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("roomgetter", db.pool()).await;

    // Create a room
    let body = serde_json::json!({"name": "Test Room"}).to_string();
    let request = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    let room_id = created["id"].as_str().expect("room_id");

    // Get it
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{room_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    assert_eq!(json["name"], "Test Room");
}

#[shared_runtime_test]
async fn test_get_room_not_found() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_rooms_pool(db.pool().clone())
        .build();

    let fake_id = uuid::Uuid::new_v4();
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{fake_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Poll CRUD ───────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_create_poll_and_add_dimension() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("pollcreator", db.pool()).await;

    // Create room
    let body = serde_json::json!({"name": "Poll Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    let room = json_body(response).await;
    let room_id = room["id"].as_str().expect("room_id");

    // Create poll
    let poll_body = serde_json::json!({
        "question": "What is the best approach to climate policy?",
        "description": "Rate each dimension."
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let poll = json_body(response).await;
    let poll_id = poll["id"].as_str().expect("poll_id");
    assert_eq!(
        poll["question"],
        "What is the best approach to climate policy?"
    );
    assert_eq!(poll["status"], "draft");

    // Add dimension
    let dim_body = serde_json::json!({
        "name": "Effectiveness",
        "description": "How effective is this approach?",
        "min_value": 0.0,
        "max_value": 10.0,
        "sort_order": 0,
        "min_label": "Not effective",
        "max_label": "Highly effective"
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let dim = json_body(response).await;
    assert_eq!(dim["name"], "Effectiveness");
    assert_eq!(dim["min_label"], "Not effective");
    assert_eq!(dim["max_label"], "Highly effective");

    // Get poll detail (includes dimensions)
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{room_id}/polls/{poll_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let detail = json_body(response).await;
    assert_eq!(detail["poll"]["id"], poll_id);
    assert_eq!(detail["dimensions"].as_array().expect("dims").len(), 1);
    assert_eq!(detail["dimensions"][0]["min_label"], "Not effective");
    assert_eq!(detail["dimensions"][0]["max_label"], "Highly effective");
}

#[shared_runtime_test]
async fn test_activate_and_close_poll() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("pollstatus", db.pool()).await;

    // Create room + poll
    let body = serde_json::json!({"name": "Status Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    let poll_body = serde_json::json!({"question": "Test?"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");
    assert_eq!(poll["status"], "draft");

    // Activate
    let status_body = serde_json::json!({"status": "active"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/status"),
        &status_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Close
    let status_body = serde_json::json!({"status": "closed"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/status"),
        &status_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

// ─── Voting ──────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_cast_vote_eligible_user() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("voter1", db.pool()).await;

    // Create room + poll + dimension
    let body = serde_json::json!({"name": "Vote Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    let poll_body = serde_json::json!({"question": "Rate this"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");

    let dim_body =
        serde_json::json!({"name": "Quality", "min_value": 0.0, "max_value": 1.0}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let dim = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let dim_id = dim["id"].as_str().expect("dim_id");

    // Activate poll
    let status_body = serde_json::json!({"status": "active"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/status"),
        &status_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    assert_eq!(
        app.clone().oneshot(req).await.expect("response").status(),
        StatusCode::NO_CONTENT
    );

    // Seed trust score so user is eligible to vote in this room
    let room_uuid: uuid::Uuid = room_id.parse().expect("room uuid");
    make_eligible(db.pool(), account_id, room_uuid).await;

    // Cast vote
    let vote_body = serde_json::json!({
        "votes": [{"dimension_id": dim_id, "value": 0.75}]
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/vote"),
        &vote_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let votes = json_body(response).await;
    let votes_array = votes.as_array().expect("array");
    assert_eq!(votes_array.len(), 1);
    assert_eq!(votes_array[0]["dimension_id"], dim_id);

    // Get my votes
    let req = build_authed_request(
        Method::GET,
        &format!("/rooms/{room_id}/polls/{poll_id}/my-votes"),
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let my_votes = json_body(response).await;
    assert_eq!(my_votes.as_array().expect("array").len(), 1);

    // Get results
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{room_id}/polls/{poll_id}/results"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let results = json_body(response).await;
    assert_eq!(results["voter_count"], 1);
}

#[shared_runtime_test]
async fn test_cast_vote_ineligible_user_returns_403() {
    let db = isolated_db().await;
    let (app, keys, _account_id) = signup_and_get_account("unverified", db.pool()).await;

    // Create room + poll + dimension + activate
    let body = serde_json::json!({"name": "Gated Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    // Set a valid constraint config so build_constraint succeeds (user has no endorsement → 403)
    let room_uuid: uuid::Uuid = room_id.parse().expect("room uuid");
    set_room_anchor(db.pool(), room_uuid).await;

    let poll_body = serde_json::json!({"question": "Test?"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");

    let dim_body =
        serde_json::json!({"name": "Score", "min_value": 0.0, "max_value": 1.0}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let dim = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let dim_id = dim["id"].as_str().expect("dim_id");

    let status_body = serde_json::json!({"status": "active"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/status"),
        &status_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    app.clone().oneshot(req).await.expect("response");

    // No trust score for the anchor — should get 403
    let vote_body = serde_json::json!({
        "votes": [{"dimension_id": dim_id, "value": 0.5}]
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/vote"),
        &vote_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body = json_body(response).await;
    assert!(body["error"].as_str().expect("error").contains("verified"));
}

#[shared_runtime_test]
async fn test_cast_vote_on_draft_poll_returns_409() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("draftvote", db.pool()).await;

    // Create room + poll + dimension (but don't activate)
    let body = serde_json::json!({"name": "Draft Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    let poll_body = serde_json::json!({"question": "Draft Poll?"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");

    let dim_body =
        serde_json::json!({"name": "Score", "min_value": 0.0, "max_value": 1.0}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let dim = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let dim_id = dim["id"].as_str().expect("dim_id");

    // Seed trust score so eligibility passes (poll status is the gate, not eligibility)
    let room_uuid: uuid::Uuid = room_id.parse().expect("room uuid");
    make_eligible(db.pool(), account_id, room_uuid).await;

    // Try to vote on a draft poll
    let vote_body = serde_json::json!({
        "votes": [{"dimension_id": dim_id, "value": 0.5}]
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/vote"),
        &vote_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[shared_runtime_test]
async fn test_vote_value_out_of_range_returns_400() {
    let db = isolated_db().await;
    let (app, keys, account_id) = signup_and_get_account("rangetest", db.pool()).await;

    // Set up room + poll + dimension + activate
    let body = serde_json::json!({"name": "Range Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    let poll_body = serde_json::json!({"question": "Range?"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");

    let dim_body =
        serde_json::json!({"name": "Score", "min_value": 0.0, "max_value": 1.0}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let dim = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let dim_id = dim["id"].as_str().expect("dim_id");

    let status_body = serde_json::json!({"status": "active"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/status"),
        &status_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    app.clone().oneshot(req).await.expect("response");

    // Seed trust score so eligibility passes (value range is the gate)
    let room_uuid: uuid::Uuid = room_id.parse().expect("room uuid");
    make_eligible(db.pool(), account_id, room_uuid).await;

    // Vote with out-of-range value
    let vote_body = serde_json::json!({
        "votes": [{"dimension_id": dim_id, "value": 999.0}]
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/vote"),
        &vote_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ─── Results ─────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_poll_results_with_multiple_voters() {
    let db = isolated_db().await;

    // Sign up two users
    let (app, keys1, account_id1) = signup_and_get_account("voter_a", db.pool()).await;
    let (_, keys2, account_id2) = signup_and_get_account("voter_b", db.pool()).await;

    // Create room + poll + dimension + activate
    let body = serde_json::json!({"name": "Results Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys1.device_signing_key,
        &keys1.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    let poll_body = serde_json::json!({"question": "Multi-voter poll"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys1.device_signing_key,
        &keys1.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");

    let dim_body =
        serde_json::json!({"name": "Rating", "min_value": 0.0, "max_value": 10.0}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys1.device_signing_key,
        &keys1.device_kid,
    );
    let dim = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let dim_id = dim["id"].as_str().expect("dim_id");

    let status_body = serde_json::json!({"status": "active"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/status"),
        &status_body,
        &keys1.device_signing_key,
        &keys1.device_kid,
    );
    app.clone().oneshot(req).await.expect("response");

    // Seed trust scores so both users are eligible to vote in this room
    let room_uuid: uuid::Uuid = room_id.parse().expect("room uuid");
    make_eligible(db.pool(), account_id1, room_uuid).await;
    make_eligible(db.pool(), account_id2, room_uuid).await;

    // Voter 1 votes 8.0
    let vote_body =
        serde_json::json!({"votes": [{"dimension_id": dim_id, "value": 8.0}]}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/vote"),
        &vote_body,
        &keys1.device_signing_key,
        &keys1.device_kid,
    );
    assert_eq!(
        app.clone().oneshot(req).await.expect("response").status(),
        StatusCode::OK
    );

    // Voter 2 votes 4.0
    let vote_body =
        serde_json::json!({"votes": [{"dimension_id": dim_id, "value": 4.0}]}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/vote"),
        &vote_body,
        &keys2.device_signing_key,
        &keys2.device_kid,
    );
    assert_eq!(
        app.clone().oneshot(req).await.expect("response").status(),
        StatusCode::OK
    );

    // Get results
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{room_id}/polls/{poll_id}/results"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let results = json_body(response).await;
    assert_eq!(results["voter_count"], 2);

    let dims = results["dimensions"].as_array().expect("dimensions");
    assert_eq!(dims.len(), 1);
    assert_eq!(dims[0]["count"], 2);
    // Mean of 8.0 and 4.0 = 6.0
    let mean = dims[0]["mean"].as_f64().expect("mean");
    assert!((mean - 6.0).abs() < 0.01, "expected mean ~6.0, got {mean}");
}

// ─── Endorsement check endpoint ──────────────────────────────────────────────

#[shared_runtime_test]
async fn test_endorsement_check_endpoint() {
    let db = isolated_db().await;
    let (app, _keys, account_id) = signup_and_get_account("endorsecheck", db.pool()).await;

    // Check before endorsement
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/endorsements/check?subject_id={account_id}&topic=identity_verified"
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["has_endorsement"], false);

    // Endorse
    endorse_user(db.pool(), account_id, "identity_verified").await;

    // Check after endorsement
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/endorsements/check?subject_id={account_id}&topic=identity_verified"
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["has_endorsement"], true);
}

// ─── Evidence in poll detail ──────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_poll_detail_includes_evidence() {
    use tinycongress_api::rooms::repo::evidence::{insert_evidence, NewEvidence};

    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("evidenceuser", db.pool()).await;

    // Create room
    let body = serde_json::json!({"name": "Evidence Room"}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let room = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let room_id = room["id"].as_str().expect("room_id");

    // Create poll
    let poll_body = serde_json::json!({"question": "Rate this company"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let poll = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let poll_id = poll["id"].as_str().expect("poll_id");

    // Add dimension
    let dim_body = serde_json::json!({
        "name": "Labor Practices",
        "min_value": 0.0,
        "max_value": 1.0,
        "min_label": "Exploitative",
        "max_label": "Exemplary"
    })
    .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/dimensions"),
        &dim_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let dim = json_body(app.clone().oneshot(req).await.expect("response")).await;
    let dim_id: uuid::Uuid = dim["id"].as_str().expect("dim_id").parse().expect("uuid");

    // Insert evidence directly via repo
    insert_evidence(
        db.pool(),
        dim_id,
        &[
            NewEvidence {
                stance: "pro",
                claim: "Offers above-minimum wages",
                source: Some("https://example.com/wages"),
            },
            NewEvidence {
                stance: "con",
                claim: "Anti-union policies documented",
                source: None,
            },
        ],
    )
    .await
    .expect("insert evidence");

    // GET poll detail
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{room_id}/polls/{poll_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let detail = json_body(response).await;
    assert_eq!(detail["poll"]["id"], poll_id);

    let dims = detail["dimensions"].as_array().expect("dimensions array");
    assert_eq!(dims.len(), 1);

    let evidence = dims[0]["evidence"].as_array().expect("evidence array");
    assert_eq!(evidence.len(), 2, "expected 2 evidence items");

    // Evidence is ordered by stance DESC (pro before con), then created_at
    let pro_item = evidence.iter().find(|e| e["stance"] == "pro").expect("pro");
    assert_eq!(pro_item["claim"], "Offers above-minimum wages");
    assert_eq!(pro_item["source"], "https://example.com/wages");
    assert!(pro_item["id"].is_string());

    let con_item = evidence.iter().find(|e| e["stance"] == "con").expect("con");
    assert_eq!(con_item["claim"], "Anti-union policies documented");
    assert!(con_item["source"].is_null());
}

// ─── Suggestions ─────────────────────────────────────────────────────────────

/// Helper: create a room and a poll within it, returning (room_id, poll_id).
async fn create_room_and_poll_for_suggestions(
    app: &axum::Router,
    keys: &common::factories::SignupKeys,
    name: &str,
) -> (String, String) {
    let body = serde_json::json!({"name": name}).to_string();
    let req = build_authed_request(
        Method::POST,
        "/rooms",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = json_body(response).await;
    let room_id = json["id"].as_str().expect("room id").to_string();

    let poll_body = serde_json::json!({"question": "What should we research next?"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls"),
        &poll_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = json_body(response).await;
    let poll_id = json["id"].as_str().expect("poll id").to_string();

    (room_id, poll_id)
}

#[shared_runtime_test]
async fn test_create_suggestion() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("suggestor1", db.pool()).await;
    let (room_id, poll_id) =
        create_room_and_poll_for_suggestions(&app, &keys, "Suggestion Room 1").await;

    let body = serde_json::json!({"suggestion_text": "Investigate renewable energy subsidies"})
        .to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/suggestions"),
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = json_body(response).await;
    assert_eq!(json["status"], "queued");
    assert_eq!(
        json["suggestion_text"],
        "Investigate renewable energy subsidies"
    );
    assert_eq!(json["room_id"], room_id.as_str());
    assert!(json["id"].is_string());
}

#[shared_runtime_test]
async fn test_create_suggestion_empty_text() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("suggestor2", db.pool()).await;
    let (room_id, poll_id) =
        create_room_and_poll_for_suggestions(&app, &keys, "Suggestion Room 2").await;

    let body = serde_json::json!({"suggestion_text": ""}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/suggestions"),
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn test_create_suggestion_too_long() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("suggestor3", db.pool()).await;
    let (room_id, poll_id) =
        create_room_and_poll_for_suggestions(&app, &keys, "Suggestion Room 3").await;

    // 501 characters — one over the 500-char limit
    let long_text = "a".repeat(501);
    let body = serde_json::json!({"suggestion_text": long_text}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/suggestions"),
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn test_list_suggestions() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("suggestor4", db.pool()).await;
    let (room_id, poll_id) =
        create_room_and_poll_for_suggestions(&app, &keys, "Suggestion Room 4").await;

    // Submit a suggestion
    let body =
        serde_json::json!({"suggestion_text": "Study housing affordability metrics"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/suggestions"),
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let create_response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created = json_body(create_response).await;
    let suggestion_id = created["id"].as_str().expect("id").to_string();

    // List suggestions
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/rooms/{room_id}/polls/{poll_id}/suggestions"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    let items = json.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], suggestion_id.as_str());
    assert_eq!(
        items[0]["suggestion_text"],
        "Study housing affordability metrics"
    );
    assert_eq!(items[0]["status"], "queued");
}

#[shared_runtime_test]
async fn test_suggestion_rate_limit() {
    let db = isolated_db().await;
    let (app, keys, _) = signup_and_get_account("suggestor5", db.pool()).await;
    let (room_id, poll_id) =
        create_room_and_poll_for_suggestions(&app, &keys, "Suggestion Room 5").await;

    // Submit 3 suggestions (the daily limit)
    for i in 0..3 {
        let body = serde_json::json!({"suggestion_text": format!("Research topic number {i}")})
            .to_string();
        let req = build_authed_request(
            Method::POST,
            &format!("/rooms/{room_id}/polls/{poll_id}/suggestions"),
            &body,
            &keys.device_signing_key,
            &keys.device_kid,
        );
        let response = app.clone().oneshot(req).await.expect("response");
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "suggestion {i} should succeed"
        );
    }

    // 4th suggestion should be rejected with 400
    let body =
        serde_json::json!({"suggestion_text": "This one should be rate limited"}).to_string();
    let req = build_authed_request(
        Method::POST,
        &format!("/rooms/{room_id}/polls/{poll_id}/suggestions"),
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = json_body(response).await;
    assert!(
        json["error"].as_str().expect("error").contains("limit"),
        "expected rate limit message, got: {}",
        json["error"]
    );
}
