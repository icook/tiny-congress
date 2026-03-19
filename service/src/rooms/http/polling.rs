//! HTTP handlers for poll-specific operations (polls, dimensions, votes, evidence)

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::http::{internal_error, not_found, ErrorResponse};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::rooms::repo::evidence;
use crate::rooms::service::{CastVoteRequest, PollError, PollingService, VoteError};

// ─── Response types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PollResponse {
    pub id: Uuid,
    pub room_id: Uuid,
    pub question: String,
    pub description: Option<String>,
    pub status: String,
    pub closes_at: Option<String>,
    pub activated_at: Option<String>,
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
#[serde(rename_all = "lowercase")]
pub enum PollStatusTransition {
    Active,
    Closed,
}

#[derive(Debug, Deserialize)]
pub struct PollStatusRequest {
    pub status: PollStatusTransition,
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

// ─── Poll handlers ─────────────────────────────────────────────────────────

pub async fn get_agenda(
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

pub async fn list_polls(
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

pub async fn get_poll_detail(
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
            tracing::error!("Failed to fetch evidence: {e}");
            return internal_error();
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

pub async fn create_poll(
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

pub async fn update_poll_status(
    Extension(polling): Extension<Arc<dyn PollingService>>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: PollStatusRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let result = match req.status {
        PollStatusTransition::Active => polling.activate_poll(poll_id).await,
        PollStatusTransition::Closed => polling.close_poll(poll_id).await,
    };
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => poll_error_response(e),
    }
}

pub async fn add_dimension(
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

pub async fn create_evidence(
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
            tracing::error!("DB error checking dimension ownership: {e}");
            return internal_error();
        }
    };

    if belongs.is_none() {
        return not_found("Dimension not found for this poll");
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
        Err(e) => {
            tracing::error!("Failed to insert evidence: {e}");
            internal_error()
        }
    }
}

pub async fn delete_evidence(
    Extension(pool): Extension<PgPool>,
    Path((_room_id, poll_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    match evidence::delete_evidence_for_poll(&pool, poll_id).await {
        Ok(deleted) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": deleted })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to delete evidence: {e}");
            internal_error()
        }
    }
}

/// Sim-only: reset a poll back to draft status, clearing all timing fields.
/// Used by the ring buffer refill logic to recycle polls for a new cycle.
pub async fn reset_poll(
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

pub async fn cast_vote(
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

pub async fn get_results(
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

pub async fn get_distribution(
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
                                    label: format!("{pct_start}\u{2013}{pct_end}%"),
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

pub async fn my_votes(
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

fn poll_to_response(p: crate::rooms::repo::PollRecord) -> PollResponse {
    PollResponse {
        id: p.id,
        room_id: p.room_id,
        question: p.question,
        description: p.description,
        status: p.status,
        closes_at: p.closes_at.map(|t| t.to_rfc3339()),
        activated_at: p.activated_at.map(|t| t.to_rfc3339()),
        created_at: p.created_at.to_rfc3339(),
    }
}

fn dim_to_response(d: crate::rooms::repo::DimensionRecord) -> DimensionResponse {
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

fn poll_error_response(e: PollError) -> axum::response::Response {
    match e {
        PollError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        PollError::PollNotFound => not_found("Poll not found"),
        PollError::Internal(_) => internal_error(),
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
        VoteError::PollNotFound => not_found("Poll not found"),
        VoteError::PollNotActive => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Poll is not currently active".to_string(),
            }),
        )
            .into_response(),
        VoteError::Internal(_) => internal_error(),
    }
}
