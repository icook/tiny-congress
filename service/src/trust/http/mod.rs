//! HTTP handlers for trust system — endorsements, denouncements, invites, scores, budget.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::repo::{TrustRepo, TrustRepoError};
use super::service::{TrustService, TrustServiceError};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::http::ErrorResponse;

// ─── Request types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EndorseRequest {
    pub subject_id: Uuid,
    #[serde(default = "default_weight")]
    pub weight: f32,
    pub attestation: Option<serde_json::Value>,
}

const fn default_weight() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct RevokeRequest {
    pub subject_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct DenounceRequest {
    pub target_id: Uuid,
    pub reason: String,
    pub influence_cost: f32,
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    pub envelope: String, // base64url-encoded bytes
    pub delivery_method: String,
    pub attestation: serde_json::Value,
}

// ─── Response types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ScoreSnapshotResponse {
    pub context_user_id: Option<Uuid>,
    pub trust_distance: Option<f32>,
    pub path_diversity: Option<i32>,
    pub eigenvector_centrality: Option<f32>,
    pub computed_at: String,
}

#[derive(Debug, Serialize)]
pub struct ScoresResponse {
    pub scores: Vec<ScoreSnapshotResponse>,
}

#[derive(Debug, Serialize)]
pub struct BudgetResponse {
    pub total_influence: f32,
    pub staked_influence: f32,
    pub spent_influence: f32,
    pub available_influence: f32,
}

#[derive(Debug, Serialize)]
pub struct CreateInviteResponse {
    pub id: Uuid,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct InviteResponse {
    pub id: Uuid,
    pub delivery_method: String,
    pub accepted_by: Option<Uuid>,
    pub expires_at: String,
    pub accepted_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvitesResponse {
    pub invites: Vec<InviteResponse>,
}

#[derive(Debug, Serialize)]
pub struct AcceptInviteResponse {
    pub endorser_id: Uuid,
    pub accepted_at: String,
}

// ─── Router ────────────────────────────────────────────────────────────────

pub fn trust_router() -> Router {
    Router::new()
        .route("/trust/endorse", post(endorse_handler))
        .route("/trust/revoke", post(revoke_handler))
        .route("/trust/denounce", post(denounce_handler))
        .route("/trust/scores/me", get(scores_me_handler))
        .route("/trust/budget", get(budget_handler))
        .route("/trust/invites", post(create_invite_handler))
        .route("/trust/invites/mine", get(list_invites_handler))
        .route("/trust/invites/{id}/accept", post(accept_invite_handler))
}

// ─── Handlers ──────────────────────────────────────────────────────────────

async fn endorse_handler(
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: EndorseRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    match trust_service
        .endorse(
            auth.account_id,
            body.subject_id,
            body.weight,
            body.attestation,
        )
        .await
    {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(MessageResponse {
                message: "endorsement queued".to_string(),
            }),
        )
            .into_response(),
        Err(ref e) => trust_service_error_response(e),
    }
}

async fn revoke_handler(
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: RevokeRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    match trust_service
        .revoke_endorsement(auth.account_id, body.subject_id)
        .await
    {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(MessageResponse {
                message: "revocation queued".to_string(),
            }),
        )
            .into_response(),
        Err(ref e) => trust_service_error_response(e),
    }
}

async fn denounce_handler(
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: DenounceRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    match trust_service
        .denounce(
            auth.account_id,
            body.target_id,
            &body.reason,
            body.influence_cost,
        )
        .await
    {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(MessageResponse {
                message: "denouncement queued".to_string(),
            }),
        )
            .into_response(),
        Err(ref e) => trust_service_error_response(e),
    }
}

async fn scores_me_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match trust_repo.get_all_scores(auth.account_id).await {
        Ok(snapshots) => {
            let scores = snapshots
                .into_iter()
                .map(|s| ScoreSnapshotResponse {
                    context_user_id: s.context_user_id,
                    trust_distance: s.trust_distance,
                    path_diversity: s.path_diversity,
                    eigenvector_centrality: s.eigenvector_centrality,
                    computed_at: s.computed_at.to_rfc3339(),
                })
                .collect();
            (StatusCode::OK, Json(ScoresResponse { scores })).into_response()
        }
        Err(ref e) => trust_repo_error_response(e),
    }
}

async fn budget_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match trust_repo.get_or_create_influence(auth.account_id).await {
        Ok(influence) => {
            let available =
                influence.total_influence - influence.staked_influence - influence.spent_influence;
            (
                StatusCode::OK,
                Json(BudgetResponse {
                    total_influence: influence.total_influence,
                    staked_influence: influence.staked_influence,
                    spent_influence: influence.spent_influence,
                    available_influence: available,
                }),
            )
                .into_response()
        }
        Err(ref e) => trust_repo_error_response(e),
    }
}

async fn create_invite_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: CreateInviteRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    let Ok(envelope_bytes) = tc_crypto::decode_base64url(&body.envelope) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid base64url encoding for envelope".to_string(),
            }),
        )
            .into_response();
    };

    let expires_at = Utc::now() + Duration::days(7);

    match trust_repo
        .create_invite(
            auth.account_id,
            &envelope_bytes,
            &body.delivery_method,
            &body.attestation,
            expires_at,
        )
        .await
    {
        Ok(invite) => (
            StatusCode::CREATED,
            Json(CreateInviteResponse {
                id: invite.id,
                expires_at: invite.expires_at.to_rfc3339(),
            }),
        )
            .into_response(),
        Err(ref e) => trust_repo_error_response(e),
    }
}

async fn list_invites_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match trust_repo.list_invites_by_endorser(auth.account_id).await {
        Ok(records) => {
            let invites = records
                .into_iter()
                .map(|r| InviteResponse {
                    id: r.id,
                    delivery_method: r.delivery_method,
                    accepted_by: r.accepted_by,
                    expires_at: r.expires_at.to_rfc3339(),
                    accepted_at: r.accepted_at.map(|t| t.to_rfc3339()),
                })
                .collect();
            (StatusCode::OK, Json(InvitesResponse { invites })).into_response()
        }
        Err(ref e) => trust_repo_error_response(e),
    }
}

async fn accept_invite_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    Path(invite_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match trust_repo.accept_invite(invite_id, auth.account_id).await {
        Ok(invite) => {
            let accepted_at = match invite.accepted_at {
                Some(t) => t.to_rfc3339(),
                None => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": "invite accepted_at not set after acceptance"
                        })),
                    )
                        .into_response()
                }
            };
            (
                StatusCode::OK,
                Json(AcceptInviteResponse {
                    endorser_id: invite.endorser_id,
                    accepted_at,
                }),
            )
                .into_response()
        }
        Err(TrustRepoError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Invite not found".to_string(),
            }),
        )
            .into_response(),
        Err(ref e) => trust_repo_error_response(e),
    }
}

// ─── Error mapping ─────────────────────────────────────────────────────────

fn trust_service_error_response(e: &TrustServiceError) -> axum::response::Response {
    match e {
        TrustServiceError::SelfAction => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Cannot target yourself".to_string(),
            }),
        )
            .into_response(),
        TrustServiceError::QuotaExceeded => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: "Daily action quota exceeded".to_string(),
            }),
        )
            .into_response(),
        TrustServiceError::InsufficientBudget => (
            StatusCode::PAYMENT_REQUIRED,
            Json(ErrorResponse {
                error: "Insufficient influence budget".to_string(),
            }),
        )
            .into_response(),
        TrustServiceError::DenouncementSlotsExhausted { max } => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: format!("Denouncement slots exhausted (max {max})"),
            }),
        )
            .into_response(),
        TrustServiceError::Repo(ref inner) => {
            tracing::error!("Trust service repo error: {inner}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response()
        }
    }
}

fn trust_repo_error_response(e: &TrustRepoError) -> axum::response::Response {
    match e {
        TrustRepoError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Not found".to_string(),
            }),
        )
            .into_response(),
        TrustRepoError::Duplicate => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Duplicate entry".to_string(),
            }),
        )
            .into_response(),
        TrustRepoError::InsufficientBudget => (
            StatusCode::PAYMENT_REQUIRED,
            Json(ErrorResponse {
                error: "Insufficient influence budget".to_string(),
            }),
        )
            .into_response(),
        TrustRepoError::Database(ref inner) => {
            tracing::error!("Trust repo database error: {inner}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response()
        }
    }
}
