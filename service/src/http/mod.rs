//! HTTP utilities and middleware.
//!
//! This module provides shared HTTP functionality used by the application server.

pub mod rate_limit;
pub mod security;

pub use security::{build_security_headers, security_headers_middleware};

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

/// Error response body shared by all HTTP handlers.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[must_use]
pub fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

#[must_use]
pub fn not_found(msg: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

#[must_use]
pub fn unauthorized(msg: &str) -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

#[must_use]
pub fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
        }),
    )
        .into_response()
}

/// 409 Conflict response with a JSON error body.
#[must_use]
pub fn conflict(msg: &str) -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

/// 403 Forbidden response with a JSON error body.
#[must_use]
pub fn forbidden(msg: &str) -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

/// 429 Too Many Requests response with a JSON error body.
#[must_use]
pub fn too_many_requests(msg: &str) -> axum::response::Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}
