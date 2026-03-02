//! Signup integration helpers for tests that need a fully registered user.
//!
//! These combine [`isolated_db`], [`TestAppBuilder`], and [`valid_signup_with_keys`]
//! into a single call that returns an app router, signing keys, and (optionally)
//! the isolated database handle.

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::common::app_builder::TestAppBuilder;
use crate::common::factories::{valid_signup_with_keys, SignupKeys};
use crate::common::test_db::{isolated_db, IsolatedDb};

/// Sign up a new user with an isolated database.
///
/// Creates a fresh DB, builds an app, performs signup, and returns all three
/// for use in subsequent authenticated requests.
pub async fn signup_user(username: &str) -> (axum::Router, SignupKeys, IsolatedDb) {
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

/// Sign up a user into an existing database pool.
///
/// Use when you need multiple users in the same database (e.g., cross-account
/// authorization tests).
pub async fn signup_user_in_pool(username: &str, pool: &PgPool) -> (axum::Router, SignupKeys) {
    let app = TestAppBuilder::new()
        .with_identity_pool(pool.clone())
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

    (app, keys)
}
