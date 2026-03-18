//! HTTP handlers for rooms and polling

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::repo::evidence;
use super::service::{
    CastVoteRequest, PollError, PollingService, RoomError, RoomsService, VoteError,
};
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
    pub poll_duration_secs: Option<i32>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct PollResponse {
    pub id: Uuid,
    pub room_id: Uuid,
    pub question: String,
    pub description: Option<String>,
    pub status: String,
    pub closes_at: Option<String>,
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
    pub min_label: Option<String>,
    pub max_label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EvidenceResponse {
    pub id: String,
    pub stance: String,
    pub claim: String,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DimensionDetailResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub min_value: f32,
    pub max_value: f32,
    pub sort_order: i32,
    pub min_label: Option<String>,
    pub max_label: Option<String>,
    pub evidence: Vec<EvidenceResponse>,
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
    pub poll_duration_secs: Option<i32>,
    #[serde(default = "default_constraint_type")]
    pub constraint_type: String,
    #[serde(default)]
    pub constraint_config: serde_json::Value,
}

fn default_eligibility_topic() -> String {
    "identity_verified".to_string()
}

fn default_constraint_type() -> String {
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
    pub min_label: Option<String>,
    pub max_label: Option<String>,
}

const fn default_max_value() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct PollStatusRequest {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateEvidenceBody {
    pub evidence: Vec<EvidenceItem>,
}

#[derive(Debug, Deserialize)]
pub struct EvidenceItem {
    pub stance: String,
    pub claim: String,
    pub source: Option<String>,
}

// ─── Router ────────────────────────────────────────────────────────────────

pub fn router() -> Router {
    Router::new()
        // Room endpoints
        .route("/rooms", get(list_rooms).post(create_room))
        .route("/rooms/capacity", get(get_capacity))
        .route("/rooms/{room_id}", get(get_room))
        .route("/rooms/{room_id}/agenda", get(get_agenda))
        // Poll endpoints
        .route("/rooms/{room_id}/polls", get(list_polls).post(create_poll))
        .route("/rooms/{room_id}/polls/{poll_id}", get(get_poll_detail))
        .route("/rooms/{room_id}/polls/{poll_id}/status", post(update_poll_status))
        .route("/rooms/{room_id}/polls/{poll_id}/dimensions", post(add_dimension))
        // Evidence endpoints
        .route(
            "/rooms/{room_id}/polls/{poll_id}/dimensions/{dimension_id}/evidence",
            post(create_evidence),
        )
        .route(
            "/rooms/{room_id}/polls/{poll_id}/evidence",
            delete(delete_evidence),
        )
        // Sim-only: ring buffer reset
        .route(
            "/rooms/{room_id}/polls/{poll_id}/reset",
            patch(reset_poll),
        )
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
            req.poll_duration_secs,
            &req.constraint_type,
            &req.constraint_config,
        )
        .await
    {
        Ok(room) => (StatusCode::CREATED, Json(room_to_response(room))).into_response(),
        Err(e) => room_error_response(e),
    }
}

async fn get_capacity(Extension(service): Extension<Arc<dyn RoomsService>>) -> impl IntoResponse {
    match service.rooms_needing_content().await {
        Ok(rooms) => {
            let rooms: Vec<_> = rooms.into_iter().map(room_to_response).collect();
            (StatusCode::OK, Json(rooms)).into_response()
        }
        Err(e) => room_error_response(e),
    }
}

async fn get_agenda(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match polling.get_agenda(room_id).await {
        Ok(polls) => {
            let polls: Vec<_> = polls.into_iter().map(poll_to_response).collect();
            (StatusCode::OK, Json(polls)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}

// ─── Poll handlers ─────────────────────────────────────────────────────────

async fn list_polls(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path(room_id): Path<Uuid>,
) -> impl IntoResponse {
    match polling.list_polls(room_id).await {
        Ok(polls) => {
            let polls: Vec<_> = polls.into_iter().map(poll_to_response).collect();
            (StatusCode::OK, Json(polls)).into_response()
        }
        Err(e) => poll_error_response(e),
    }
}

async fn get_poll_detail(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Extension(pool): Extension<PgPool>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let poll = match polling.get_poll(poll_id).await {
        Ok(p) => p,
        Err(e) => return poll_error_response(e),
    };
    let dimensions = match polling.list_dimensions(poll_id).await {
        Ok(d) => d,
        Err(e) => return poll_error_response(e),
    };

    let dimension_ids: Vec<Uuid> = dimensions.iter().map(|d| d.id).collect();
    let evidence_records = match evidence::get_evidence_for_dimensions(&pool, &dimension_ids).await
    {
        Ok(ev) => ev,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::identity::http::ErrorResponse {
                    error: format!("Failed to fetch evidence: {e}"),
                }),
            )
                .into_response()
        }
    };

    let mut evidence_by_dim: HashMap<Uuid, Vec<EvidenceResponse>> = HashMap::new();
    for ev in evidence_records {
        evidence_by_dim
            .entry(ev.dimension_id)
            .or_default()
            .push(EvidenceResponse {
                id: ev.id.to_string(),
                stance: ev.stance,
                claim: ev.claim,
                source: ev.source,
            });
    }

    let response = PollDetailResponse {
        poll: poll_to_response(poll),
        dimensions: dimensions
            .into_iter()
            .map(|d| {
                let ev = evidence_by_dim.remove(&d.id).unwrap_or_default();
                DimensionDetailResponse {
                    id: d.id,
                    name: d.name,
                    description: d.description,
                    min_value: d.min_value,
                    max_value: d.max_value,
                    sort_order: d.sort_order,
                    min_label: d.min_label,
                    max_label: d.max_label,
                    evidence: ev,
                }
            })
            .collect(),
    };
    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Serialize)]
struct PollDetailResponse {
    poll: PollResponse,
    dimensions: Vec<DimensionDetailResponse>,
}

async fn create_poll(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreatePollRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match polling
        .create_poll(room_id, &req.question, req.description.as_deref())
        .await
    {
        Ok(poll) => (StatusCode::CREATED, Json(poll_to_response(poll))).into_response(),
        Err(e) => poll_error_response(e),
    }
}

async fn update_poll_status(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: PollStatusRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let result = match req.status.as_str() {
        "active" => polling.activate_poll(poll_id).await,
        "closed" => polling.close_poll(poll_id).await,
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
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CreateDimensionRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match polling
        .add_dimension(
            poll_id,
            &req.name,
            req.description.as_deref(),
            req.min_value,
            req.max_value,
            req.sort_order,
            req.min_label.as_deref(),
            req.max_label.as_deref(),
        )
        .await
    {
        Ok(dim) => (StatusCode::CREATED, Json(dim_to_response(dim))).into_response(),
        Err(e) => poll_error_response(e),
    }
}

// ─── Evidence handlers ─────────────────────────────────────────────────────

async fn create_evidence(
    Extension(pool): Extension<PgPool>,
    Path((_room_id, poll_id, dimension_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(body): Json<CreateEvidenceBody>,
) -> impl IntoResponse {
    // Validate dimension belongs to the poll
    let belongs: Option<(Uuid,)> = match sqlx::query_as(
        "SELECT id FROM rooms__poll_dimensions WHERE id = $1 AND poll_id = $2",
    )
    .bind(dimension_id)
    .bind(poll_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("DB error: {e}") })),
            )
                .into_response()
        }
    };

    if belongs.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Dimension not found for this poll" })),
        )
            .into_response();
    }

    let new_evidence: Vec<evidence::NewEvidence<'_>> = body
        .evidence
        .iter()
        .map(|item| evidence::NewEvidence {
            stance: &item.stance,
            claim: &item.claim,
            source: item.source.as_deref(),
        })
        .collect();

    match evidence::insert_evidence(&pool, dimension_id, &new_evidence).await {
        Ok(count) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "count": count })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to insert evidence: {e}") })),
        )
            .into_response(),
    }
}

async fn delete_evidence(
    Extension(pool): Extension<PgPool>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    match evidence::delete_evidence_for_poll(&pool, poll_id).await {
        Ok(deleted) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": deleted })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Failed to delete evidence: {e}") })),
        )
            .into_response(),
    }
}

/// Sim-only: reset a poll back to draft status, clearing all timing fields.
/// Used by the ring buffer refill logic to recycle polls for a new cycle.
async fn reset_poll(
    Extension(pool): Extension<PgPool>,
    Path((room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, StatusCode> {
    let result = sqlx::query(
        "UPDATE rooms__polls \
         SET status = 'draft', closes_at = NULL, activated_at = NULL, closed_at = NULL \
         WHERE id = $1 AND room_id = $2",
    )
    .bind(poll_id)
    .bind(room_id)
    .execute(&pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => Err(StatusCode::NOT_FOUND),
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ─── Vote handlers ─────────────────────────────────────────────────────────

async fn cast_vote(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: CastVoteRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    match polling
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
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    match polling.get_poll_results(poll_id).await {
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
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    match polling.get_poll_distribution(poll_id).await {
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
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match polling.get_user_votes(poll_id, auth.account_id).await {
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
        poll_duration_secs: r.poll_duration_secs,
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
        closes_at: p.closes_at.map(|t| t.to_rfc3339()),
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
        min_label: d.min_label,
        max_label: d.max_label,
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
