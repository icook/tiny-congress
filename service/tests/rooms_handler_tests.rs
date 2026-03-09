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

    create_endorsement(pool, account_id, topic, None, None, 1.0, None)
        .await
        .expect("endorsement");
}

/// Helper: seed a global trust score snapshot that makes `account_id` eligible to vote.
///
/// The `EndorsedByConstraint` with a nil anchor checks `trust_repo.get_score(user_id, None)`.
/// Seeding a score with `trust_distance = Some(1.0)` makes the user reachable and eligible.
async fn make_eligible(pool: &sqlx::PgPool, account_id: uuid::Uuid, _room_id: uuid::Uuid) {
    use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .upsert_score(account_id, None, Some(1.0), Some(1), None)
        .await
        .expect("trust score");
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

    // No endorsement — should get 403
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
