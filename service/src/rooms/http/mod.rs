//! HTTP handlers for rooms and polling
//!
//! Organized into two submodules:
//! - `platform` — room CRUD handlers (engine-agnostic)
//! - `polling` — poll, dimension, vote, and evidence handlers (polling-engine-specific)

pub(crate) mod platform;
pub(crate) mod polling;
pub(crate) mod ranking;

use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use serde::Deserialize;
use utoipa::ToSchema;

// Re-export response/request types that external code depends on
pub use polling::{
    BucketResponse, CreateDimensionRequest, CreateEvidenceBody, CreatePollRequest,
    DimensionDetailResponse, DimensionDistributionResponse, DimensionResponse,
    DimensionStatsResponse, EvidenceItem, EvidenceResponse, PollDistributionResponse, PollResponse,
    PollResultsResponse, PollStatusRequest, VoteResponse,
};

// ─── Request types (platform-level) ───────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRoomRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_eligibility_topic")]
    pub eligibility_topic: String,
    pub poll_duration_secs: Option<i32>,
    #[serde(default = "default_constraint_type")]
    pub constraint_type: String,
    #[serde(default)]
    pub constraint_config: serde_json::Value,
    #[serde(default = "default_engine_type")]
    pub engine_type: String,
    #[serde(default)]
    pub engine_config: serde_json::Value,
}

fn default_engine_type() -> String {
    "polling".to_string()
}

fn default_eligibility_topic() -> String {
    "identity_verified".to_string()
}

fn default_constraint_type() -> String {
    "endorsed_by_user".to_string()
}

// ─── Response types (platform-level) ──────────────────────────────────────

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct RoomResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub eligibility_topic: String,
    pub status: String,
    pub poll_duration_secs: Option<i32>,
    pub created_at: String,
    pub engine_type: String,
    pub engine_config: serde_json::Value,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub owner_id: Option<uuid::Uuid>,
    pub constraint_type: String,
}

// ─── Capabilities response ────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct MyCapabilitiesResponse {
    pub role: String,
    pub can_vote: bool,
    pub can_configure: bool,
    pub reason: Option<String>,
    pub next_step: Option<String>,
}

// ─── Role assignment types ────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssignRoleRequest {
    #[schema(value_type = String, format = "uuid")]
    pub account_id: uuid::Uuid,
    pub role: String,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct AssignRoleResponse {
    #[schema(value_type = String, format = "uuid")]
    pub room_id: uuid::Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub account_id: uuid::Uuid,
    pub role: String,
}

// ─── Suggestion types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSuggestionRequest {
    pub suggestion_text: String,
}

#[derive(Debug, serde::Serialize, ToSchema)]
pub struct SuggestionResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: uuid::Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub room_id: uuid::Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub poll_id: uuid::Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub account_id: uuid::Uuid,
    pub suggestion_text: String,
    pub status: String,
    pub filter_reason: Option<String>,
    #[schema(value_type = Vec<String>)]
    pub evidence_ids: Vec<uuid::Uuid>,
    pub created_at: String,
    pub processed_at: Option<String>,
}

// ─── Router ────────────────────────────────────────────────────────────────

pub fn router() -> Router {
    Router::new()
        // Platform: room endpoints
        .route(
            "/rooms",
            get(platform::list_rooms).post(platform::create_room),
        )
        .route("/rooms/capacity", get(platform::get_capacity))
        .route("/rooms/{room_id}", get(platform::get_room))
        .route(
            "/rooms/{room_id}/my-capabilities",
            get(platform::my_capabilities),
        )
        .route("/rooms/{room_id}/roles", post(platform::assign_role))
        // Platform: suggestions
        .route(
            "/rooms/{room_id}/polls/{poll_id}/suggestions",
            get(platform::list_suggestions).post(platform::create_suggestion),
        )
        // Polling: agenda
        .route("/rooms/{room_id}/agenda", get(polling::get_agenda))
        // Polling: poll endpoints
        .route(
            "/rooms/{room_id}/polls",
            get(polling::list_polls).post(polling::create_poll),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}",
            get(polling::get_poll_detail),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/status",
            post(polling::update_poll_status),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/dimensions",
            post(polling::add_dimension),
        )
        // Polling: evidence endpoints
        .route(
            "/rooms/{room_id}/polls/{poll_id}/dimensions/{dimension_id}/evidence",
            post(polling::create_evidence),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/evidence",
            delete(polling::delete_evidence),
        )
        // Sim-only: ring buffer reset
        .route(
            "/rooms/{room_id}/polls/{poll_id}/reset",
            patch(polling::reset_poll),
        )
        // Polling: vote + results
        .route(
            "/rooms/{room_id}/polls/{poll_id}/vote",
            post(polling::cast_vote),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/results",
            get(polling::get_results),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/results/distribution",
            get(polling::get_distribution),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/my-votes",
            get(polling::my_votes),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/traces",
            get(polling::get_poll_traces),
        )
        // Ranking engine routes
        .route(
            "/rooms/{room_id}/submissions",
            post(ranking::submit_meme),
        )
        .route(
            "/rooms/{room_id}/matchup",
            get(ranking::get_matchup),
        )
        .route(
            "/rooms/{room_id}/matchups",
            post(ranking::record_matchup),
        )
        .route(
            "/rooms/{room_id}/rounds/current",
            get(ranking::get_current_rounds),
        )
        .route(
            "/rooms/{room_id}/rounds",
            get(ranking::list_rounds),
        )
        .route(
            "/rooms/{room_id}/rounds/{round_id}/leaderboard",
            get(ranking::get_leaderboard),
        )
        .route(
            "/rooms/{room_id}/hall-of-fame",
            get(ranking::get_hall_of_fame),
        )
}
