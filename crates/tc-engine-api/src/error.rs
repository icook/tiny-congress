//! Error types for room engine operations.
//!
//! [`EngineError`] is the canonical error type returned by engine plugin
//! methods. It maps cleanly to HTTP status codes via its [`IntoResponse`]
//! implementation and to RFC 7807 Problem Details for API consumers.

use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

/// Errors returned by room engine operations.
///
/// Each variant maps to a specific HTTP status code. The `Display` impl
/// produces a human-readable message suitable for API error responses.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The requested resource does not exist (404).
    #[error("{0}")]
    NotFound(String),

    /// The caller is not eligible for the requested operation (403).
    #[error("{0}")]
    NotEligible(String),

    /// The request is malformed or contains invalid parameters (400).
    #[error("{0}")]
    InvalidInput(String),

    /// The operation conflicts with current state (409).
    #[error("{0}")]
    Conflict(String),

    /// An unexpected internal error (500).
    #[error(transparent)]
    Internal(anyhow::Error),
}

impl EngineError {
    /// Returns the HTTP status code for this error variant.
    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::NotEligible(_) => StatusCode::FORBIDDEN,
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for EngineError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();

        // For internal errors, don't leak implementation details to the client.
        let detail = match &self {
            Self::Internal(_) => "Internal server error".to_string(),
            other => other.to_string(),
        };

        let body = json!({
            "type": format!("https://tinycongress.com/errors/{}", error_type_slug(&self)),
            "title": status.canonical_reason().unwrap_or("Error"),
            "status": status.as_u16(),
            "detail": detail,
        });

        (status, axum::Json(body)).into_response()
    }
}

/// Maps an `EngineError` variant to a URL-safe slug for the `type` field.
const fn error_type_slug(err: &EngineError) -> &'static str {
    match err {
        EngineError::NotFound(_) => "not-found",
        EngineError::NotEligible(_) => "not-eligible",
        EngineError::InvalidInput(_) => "invalid-input",
        EngineError::Conflict(_) => "conflict",
        EngineError::Internal(_) => "internal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_match_variants() {
        assert_eq!(
            EngineError::NotFound("x".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            EngineError::NotEligible("x".into()).status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            EngineError::InvalidInput("x".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            EngineError::Conflict("x".into()).status_code(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            EngineError::Internal(anyhow::anyhow!("x")).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn display_shows_message_for_non_internal() {
        let err = EngineError::NotFound("room 42 not found".into());
        assert_eq!(err.to_string(), "room 42 not found");
    }

    #[test]
    fn display_shows_inner_for_internal() {
        let err = EngineError::Internal(anyhow::anyhow!("db connection lost"));
        assert_eq!(err.to_string(), "db connection lost");
    }

    #[tokio::test]
    async fn into_response_masks_internal_error_detail() {
        let err = EngineError::Internal(anyhow::anyhow!("db connection lost"));
        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Internal errors MUST NOT leak implementation details to clients.
        assert_eq!(json["detail"], "Internal server error");
        assert_ne!(json["detail"], "db connection lost");
    }
}
