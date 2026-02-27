//! REST API handlers and `OpenAPI` documentation.
//!
//! This module provides REST endpoints alongside GraphQL, sharing the same
//! domain types with `ToSchema` derives for `OpenAPI` spec generation.

// The OpenApi derive macro generates code that triggers this lint
#![allow(clippy::needless_for_each)]

use crate::build_info::BuildInfo;
use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};
use serde::{Serialize, Serializer};
use utoipa::{OpenApi, ToSchema};

/// Serialize a `StatusCode` as its `u16` representation.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires `&T` signature
fn serialize_status_code<S: Serializer>(status: &StatusCode, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u16(status.as_u16())
}

/// RFC 7807 Problem Details error response.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    /// URI reference identifying the problem type
    #[serde(rename = "type")]
    pub problem_type: String,
    /// Short human-readable summary
    pub title: String,
    /// HTTP status code
    #[serde(serialize_with = "serialize_status_code")]
    #[schema(value_type = u16)]
    pub status: StatusCode,
    /// Human-readable explanation specific to this occurrence
    pub detail: String,
    /// URI reference identifying the specific occurrence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ProblemExtensions>,
}

/// Extended error information mapping to GraphQL error codes.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemExtensions {
    /// Error code matching GraphQL error codes
    pub code: String,
    /// Field that caused the error (for validation errors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

impl ProblemDetails {
    /// Create an internal server error response.
    #[must_use]
    pub fn internal_error(detail: &str) -> Self {
        Self {
            problem_type: "https://tinycongress.com/errors/internal".to_string(),
            title: "Internal Server Error".to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
            detail: detail.to_string(),
            instance: None,
            extensions: Some(ProblemExtensions {
                code: "INTERNAL_ERROR".to_string(),
                field: None,
            }),
        }
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}

/// `OpenAPI` documentation for the REST API.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "TinyCongress API",
        version = "1.0.0",
        description = "REST API for TinyCongress",
        license(name = "MIT")
    ),
    servers(
        (url = "/api/v1", description = "REST API v1")
    ),
    paths(get_build_info),
    components(schemas(BuildInfo, ProblemDetails, ProblemExtensions))
)]
pub struct ApiDoc;

/// Get build information
///
/// Returns metadata about the running service including version, git SHA, and build time.
///
/// # Errors
///
/// Returns `ProblemDetails` on internal server errors.
#[utoipa::path(
    get,
    path = "/build-info",
    tag = "System",
    responses(
        (status = 200, description = "Build information retrieved successfully", body = BuildInfo),
        (status = 500, description = "Internal server error", body = ProblemDetails)
    )
)]
#[allow(clippy::unused_async)] // Required for Axum handler signature
pub async fn get_build_info(
    Extension(build_info): Extension<BuildInfo>,
) -> Result<Json<BuildInfo>, ProblemDetails> {
    Ok(Json(build_info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_details_serializes_correctly() {
        let problem = ProblemDetails::internal_error("Something went wrong");
        let json = serde_json::to_string(&problem).expect("serialize");
        assert!(json.contains("\"type\":"));
        assert!(json.contains("INTERNAL_ERROR"));
    }
}
