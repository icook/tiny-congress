//! HTTP handlers for rooms and polling

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::service::{CastVoteRequest, PollError, RoomError, RoomsService, VoteError};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::http::ErrorResponse;

// ─── Response types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RoomResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub eligibility_topic: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct PollResponse {
    pub id: Uuid,
    pub room_id: Uuid,
    pub question: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct DimensionResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub min_value: f32,
    pub max_value: f32,
    pub sort_order: i32,
}

#[derive(Debug, Serialize)]
pub struct PollResultsResponse {
    pub poll: PollResponse,
    pub dimensions: Vec<DimensionStatsResponse>,
    pub voter_count: i64,
}

#[derive(Debug, Serialize)]
pub struct DimensionStatsResponse {
    pub dimension_id: Uuid,
    pub dimension_name: String,
    pub count: i64,
    pub mean: f64,
    pub median: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize)]
pub struct BucketResponse {
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct DimensionDistributionResponse {
    pub dimension_id: Uuid,
    pub dimension_name: String,
    pub buckets: Vec<BucketResponse>,
}

#[derive(Debug, Serialize)]
pub struct PollDistributionResponse {
    pub dimensions: Vec<DimensionDistributionResponse>,
}

#[derive(Debug, Serialize)]
pub struct VoteResponse {
    pub dimension_id: Uuid,
    pub value: f32,
    pub updated_at: String,
}

// ─── Request types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_eligibility_topic")]
    pub eligibility_topic: String,
}

fn default_eligibility_topic() -> String {
    "identity_verified".to_string()
}

#[derive(Debug, Deserialize)]
pub struct CreatePollRequest {
    pub question: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDimensionRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub min_value: f32,
    #[serde(default = "default_max_value")]
    pub max_value: f32,
    #[serde(default)]
    pub sort_order: i32,
}

const fn default_max_value() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct PollStatusRequest {
    pub status: String,
}

// ─── Router ────────────────────────────────────────────────────────────────

pub fn router() -> Router {
    Router::new()
        // Room endpoints
        .route("/rooms", get(list_rooms).post(create_room))
        .route("/rooms/{room_id}", get(get_room))
        // Poll endpoints
        .route("/rooms/{room_id}/polls", get(list_polls).post(create_poll))
        .route("/rooms/{room_id}/polls/{poll_id}", get(get_poll_detail))
        .route("/rooms/{room_id}/polls/{poll_id}/status", post(update_poll_status))
        .route("/rooms/{room_id}/polls/{poll_id}/dimensions", post(add_dimension))
        // Vote + results
        .route("/rooms/{room_id}/polls/{poll_id}/vote", post(cast_vote))
        .route("/rooms/{room_id}/polls/{poll_id}/results", get(get_results))
        .route(
            "/rooms/{room_id}/polls/{poll_id}/results/distribution",
            get(get_distribution),
        )
        .route("/rooms/{room_id}/polls/{poll_id}/my-votes", get(my_votes))
}

// ─── Room handlers ─────────────────────────────────────────────────────────

async fn list_rooms(Extension(service): Extension<Arc<dyn RoomsService>>) -> impl IntoResponse {
    match service.list_rooms(Some("open")).await {
        Ok(rooms) => {
            let rooms: Vec<_> = rooms.into_iter().map(room_to_response).collect();
            (StatusCode::OK, Json(rooms)).into_response()
        }
        Err(e) => room_error_response(e),
    }
}

async fn get_room(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match service.get_room(room_id).await {
        Ok(room) => (StatusCode::OK, Json(room_to_response(room))).into_response(),
        Err(e) => room_error_response(e),
    }
}

async fn create_room(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreateRoomRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match service
        .create_room(
            &req.name,
            req.description.as_deref(),
            &req.eligibility_topic,
        )
        .await
    {
        Ok(room) => (StatusCode::CREATED, Json(room_to_response(room))).into_response(),
        Err(e) => room_error_response(e),
    }
}

// ─── Poll handlers ─────────────────────────────────────────────────────────

async fn list_polls(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match service.list_polls(room_id).await {
        Ok(polls) => {
            let polls: Vec<_> = polls.into_iter().map(poll_to_response).collect();
            (StatusCode::OK, Json(polls)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}

async fn get_poll_detail(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let poll = match service.get_poll(poll_id).await {
        Ok(p) => p,
        Err(e) => return poll_error_response(e),
    };
    let dimensions = match service.list_dimensions(poll_id).await {
        Ok(d) => d,
        Err(e) => return poll_error_response(e),
    };

    let response = PollDetailResponse {
        poll: poll_to_response(poll),
        dimensions: dimensions.into_iter().map(dim_to_response).collect(),
    };
    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Serialize)]
struct PollDetailResponse {
    poll: PollResponse,
    dimensions: Vec<DimensionResponse>,
}

async fn create_poll(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreatePollRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match service
        .create_poll(room_id, &req.question, req.description.as_deref())
        .await
    {
        Ok(poll) => (StatusCode::CREATED, Json(poll_to_response(poll))).into_response(),
        Err(e) => poll_error_response(e),
    }
}

async fn update_poll_status(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: PollStatusRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let result = match req.status.as_str() {
        "active" => service.activate_poll(poll_id).await,
        "closed" => service.close_poll(poll_id).await,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Status must be 'active' or 'closed'".to_string(),
                }),
            )
                .into_response()
        }
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => poll_error_response(e),
    }
}

async fn add_dimension(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreateDimensionRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match service
        .add_dimension(
            poll_id,
            &req.name,
            req.description.as_deref(),
            req.min_value,
            req.max_value,
            req.sort_order,
        )
        .await
    {
        Ok(dim) => (StatusCode::CREATED, Json(dim_to_response(dim))).into_response(),
        Err(e) => poll_error_response(e),
    }
}

// ─── Vote handlers ─────────────────────────────────────────────────────────

async fn cast_vote(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CastVoteRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match service
        .cast_vote(poll_id, auth.account_id, &req.votes)
        .await
    {
        Ok(votes) => {
            let votes: Vec<_> = votes
                .into_iter()
                .map(|v| VoteResponse {
                    dimension_id: v.dimension_id,
                    value: v.value,
                    updated_at: v.updated_at.to_rfc3339(),
                })
                .collect();
            (StatusCode::OK, Json(votes)).into_response()
        }
        Err(e) => vote_error_response(e),
    }
}

async fn get_results(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    match service.get_poll_results(poll_id).await {
        Ok(results) => {
            let response = PollResultsResponse {
                poll: poll_to_response(results.poll),
                dimensions: results
                    .dimensions
                    .into_iter()
                    .map(|d| DimensionStatsResponse {
                        dimension_id: d.dimension_id,
                        dimension_name: d.dimension_name,
                        count: d.count,
                        mean: d.mean,
                        median: d.median,
                        stddev: d.stddev,
                        min: d.min,
                        max: d.max,
                    })
                    .collect(),
                voter_count: results.voter_count,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}

async fn get_distribution(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    match service.get_poll_distribution(poll_id).await {
        Ok(dist) => {
            let num_buckets = 10usize;
            let response = PollDistributionResponse {
                dimensions: dist
                    .dimensions
                    .into_iter()
                    .map(|d| DimensionDistributionResponse {
                        dimension_id: d.dimension_id,
                        dimension_name: d.dimension_name,
                        buckets: d
                            .buckets
                            .into_iter()
                            .enumerate()
                            .map(|(i, b)| {
                                let pct_start = (i * 100) / num_buckets;
                                let pct_end = ((i + 1) * 100) / num_buckets;
                                BucketResponse {
                                    label: format!("{pct_start}–{pct_end}%"),
                                    count: b.count,
                                }
                            })
                            .collect(),
                    })
                    .collect(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}

async fn my_votes(
    Extension(service): Extension<Arc<dyn RoomsService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match service.get_user_votes(poll_id, auth.account_id).await {
        Ok(votes) => {
            let votes: Vec<_> = votes
                .into_iter()
                .map(|v| VoteResponse {
                    dimension_id: v.dimension_id,
                    value: v.value,
                    updated_at: v.updated_at.to_rfc3339(),
                })
                .collect();
            (StatusCode::OK, Json(votes)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}

// ─── Response converters ──────────────────────────────────────────────────

fn room_to_response(r: super::repo::RoomRecord) -> RoomResponse {
    RoomResponse {
        id: r.id,
        name: r.name,
        description: r.description,
        eligibility_topic: r.eligibility_topic,
        status: r.status,
        created_at: r.created_at.to_rfc3339(),
    }
}

fn poll_to_response(p: super::repo::PollRecord) -> PollResponse {
    PollResponse {
        id: p.id,
        room_id: p.room_id,
        question: p.question,
        description: p.description,
        status: p.status,
        created_at: p.created_at.to_rfc3339(),
    }
}

fn dim_to_response(d: super::repo::DimensionRecord) -> DimensionResponse {
    DimensionResponse {
        id: d.id,
        name: d.name,
        description: d.description,
        min_value: d.min_value,
        max_value: d.max_value,
        sort_order: d.sort_order,
    }
}

// ─── Error mappers ────────────────────────────────────────────────────────

fn room_error_response(e: RoomError) -> axum::response::Response {
    match e {
        RoomError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        RoomError::RoomNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Room not found".to_string(),
            }),
        )
            .into_response(),
        RoomError::DuplicateRoomName => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Room name already exists".to_string(),
            }),
        )
            .into_response(),
        RoomError::Internal(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
            }),
        )
            .into_response(),
    }
}

fn poll_error_response(e: PollError) -> axum::response::Response {
    match e {
        PollError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        PollError::PollNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Poll not found".to_string(),
            }),
        )
            .into_response(),
        PollError::Internal(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
            }),
        )
            .into_response(),
    }
}

fn vote_error_response(e: VoteError) -> axum::response::Response {
    match e {
        VoteError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        VoteError::NotEligible(msg) => {
            (StatusCode::FORBIDDEN, Json(ErrorResponse { error: msg })).into_response()
        }
        VoteError::PollNotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Poll not found".to_string(),
            }),
        )
            .into_response(),
        VoteError::PollNotActive => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Poll is not currently active".to_string(),
            }),
        )
            .into_response(),
        VoteError::Internal(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
            }),
        )
            .into_response(),
    }
}
