//! HTTP utilities and middleware.
//!
//! This module provides shared HTTP functionality used by the application server.

pub mod rate_limit;
pub mod security;

pub use security::{build_security_headers, security_headers_middleware};

use axum::{
    extract::{path::ErrorKind, rejection::PathRejection, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

/// A drop-in replacement for [`axum::extract::Path`] that returns a JSON
/// [`ErrorResponse`] on rejection instead of a plain-text response.
pub struct Path<T>(pub T);

impl<S, T> FromRequestParts<S> for Path<T>
where
    T: serde::de::DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = axum::response::Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(path_rejection_to_response(&rejection)),
        }
    }
}

fn path_rejection_to_response(rejection: &PathRejection) -> axum::response::Response {
    let param_name = extract_param_name(rejection);
    let msg = format!("Invalid path parameter: {param_name}");
    bad_request(&msg)
}

fn extract_param_name(rejection: &PathRejection) -> String {
    if let PathRejection::FailedToDeserializePathParams(inner) = rejection {
        if let ErrorKind::ParseErrorAtKey { key, .. } = inner.kind() {
            return key.clone();
        }
        if let ErrorKind::ParseError { .. } = inner.kind() {
            return "value".to_string();
        }
    }
    "parameter".to_string()
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, routing::get, Router};
    use tower::ServiceExt;
    use uuid::Uuid;

    /// Verify that an invalid path parameter (unparseable as UUID) returns a
    /// JSON `ErrorResponse` with status 400 rather than a plain-text error.
    #[tokio::test]
    async fn invalid_path_param_returns_json_error() {
        let app = Router::new().route(
            "/items/{id}",
            get(|Path(id): Path<Uuid>| async move { id.to_string() }),
        );

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/items/not-a-uuid")
            .body(axum::body::Body::empty())
            .expect("request");

        let response = app.oneshot(request).await.expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        let body = to_bytes(response.into_body(), 1024).await.expect("body");
        let payload: ErrorResponse =
            serde_json::from_slice(&body).expect("valid JSON ErrorResponse");
        assert!(
            payload.error.contains("id") || payload.error.contains("parameter"),
            "error should name the param or say 'parameter', got: {}",
            payload.error
        );
    }
}
