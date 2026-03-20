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
    paths(
        get_build_info,
        crate::reputation::http::create_endorsement_as_verifier,
        crate::trust::http::budget_handler,
        crate::trust::http::endorse_handler,
        crate::trust::http::revoke_handler,
        crate::trust::http::scores_me_handler,
        crate::trust::http::create_invite_handler,
        crate::trust::http::list_invites_handler,
        crate::trust::http::accept_invite_handler,
        crate::trust::http::denounce_handler,
        crate::trust::http::list_my_denouncements_handler,
        // Identity
        crate::identity::http::signup,
        crate::identity::http::account_lookup,
        crate::identity::http::backup::get_backup,
        crate::identity::http::devices::list_devices,
        crate::identity::http::devices::add_device,
        crate::identity::http::devices::revoke_device,
        crate::identity::http::devices::rename_device,
        crate::identity::http::login::login,
        // Rooms (platform)
        crate::rooms::http::platform::list_rooms,
        crate::rooms::http::platform::get_room,
        crate::rooms::http::platform::create_room,
        crate::rooms::http::platform::get_capacity,
        crate::rooms::http::platform::my_capabilities,
        crate::rooms::http::platform::assign_role,
        crate::rooms::http::platform::create_suggestion,
        crate::rooms::http::platform::list_suggestions,
        // Rooms (polls)
        crate::rooms::http::polling::get_agenda,
        crate::rooms::http::polling::list_polls,
        crate::rooms::http::polling::get_poll_detail,
        crate::rooms::http::polling::create_poll,
        crate::rooms::http::polling::update_poll_status,
        crate::rooms::http::polling::add_dimension,
        crate::rooms::http::polling::create_evidence,
        crate::rooms::http::polling::delete_evidence,
        crate::rooms::http::polling::reset_poll,
        crate::rooms::http::polling::cast_vote,
        crate::rooms::http::polling::get_results,
        crate::rooms::http::polling::get_distribution,
        crate::rooms::http::polling::my_votes,
        crate::rooms::http::polling::get_poll_traces,
    ),
    components(schemas(
        BuildInfo,
        ProblemDetails,
        ProblemExtensions,
        crate::reputation::http::CreateEndorsementRequest,
        crate::reputation::http::CreatedEndorsementResponse,
        crate::trust::http::BudgetResponse,
        crate::trust::http::ScoreSnapshotResponse,
        crate::trust::http::ScoresResponse,
        crate::trust::http::CreateInviteResponse,
        crate::trust::http::InviteResponse,
        crate::trust::http::InvitesResponse,
        crate::trust::http::AcceptInviteResponse,
        crate::trust::http::DenouncementResponse,
        crate::trust::http::MessageResponse,
        crate::trust::http::EndorseRequest,
        crate::trust::http::RevokeRequest,
        crate::trust::http::DenounceRequest,
        crate::trust::http::CreateInviteRequest,
        // Identity schemas
        crate::identity::service::SignupRequest,
        crate::identity::service::SignupBackup,
        crate::identity::service::SignupDevice,
        crate::identity::http::SignupResponse,
        crate::identity::http::AccountLookupResponse,
        crate::identity::http::backup::BackupResponse,
        crate::identity::http::devices::DeviceInfo,
        crate::identity::http::devices::DeviceListResponse,
        crate::identity::http::devices::AddDeviceRequest,
        crate::identity::http::devices::AddDeviceResponse,
        crate::identity::http::devices::RenameDeviceRequest,
        crate::identity::http::login::LoginRequest,
        crate::identity::http::login::LoginDevice,
        crate::identity::http::login::LoginResponse,
        // Rooms schemas
        crate::rooms::http::CreateRoomRequest,
        crate::rooms::http::RoomResponse,
        crate::rooms::http::MyCapabilitiesResponse,
        crate::rooms::http::AssignRoleRequest,
        crate::rooms::http::AssignRoleResponse,
        crate::rooms::http::CreateSuggestionRequest,
        crate::rooms::http::SuggestionResponse,
        crate::rooms::http::polling::PollResponse,
        crate::rooms::http::polling::DimensionResponse,
        crate::rooms::http::polling::EvidenceResponse,
        crate::rooms::http::polling::DimensionDetailResponse,
        crate::rooms::http::polling::PollResultsResponse,
        crate::rooms::http::polling::DimensionStatsResponse,
        crate::rooms::http::polling::BucketResponse,
        crate::rooms::http::polling::DimensionDistributionResponse,
        crate::rooms::http::polling::PollDistributionResponse,
        crate::rooms::http::polling::VoteResponse,
        crate::rooms::http::polling::BotTraceResponse,
        crate::rooms::http::polling::CreatePollRequest,
        crate::rooms::http::polling::CreateDimensionRequest,
        crate::rooms::http::polling::PollStatusRequest,
        crate::rooms::http::polling::PollStatusTransition,
        crate::rooms::http::polling::CreateEvidenceBody,
        crate::rooms::http::polling::EvidenceItem,
    ))
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
