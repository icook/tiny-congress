//! HTTP handlers for trust system — endorsements, denouncements, invites, scores, budget.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::repo::{TrustRepo, TrustRepoError};
use super::service::{TrustService, TrustServiceError};
use super::weight::compute_endorsement_weight;
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::http::ErrorResponse;
use crate::reputation::repo::ReputationRepo;

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
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    pub envelope: String, // base64url-encoded bytes
    pub delivery_method: String,
    pub relationship_depth: Option<String>,
    pub weight: Option<f32>,
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
    pub slots_total: u32,
    pub slots_used: i64,
    pub slots_available: i64,
    pub denouncements_total: u32,
    pub denouncements_used: i64,
    pub denouncements_available: i64,
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

#[derive(Debug, Serialize)]
pub struct DenouncementResponse {
    pub id: Uuid,
    pub target_id: Uuid,
    pub target_username: String,
    pub reason: String,
    pub created_at: String,
}

/// Row returned by the denouncements join query.
#[derive(sqlx::FromRow)]
struct DenouncementWithUsername {
    pub id: Uuid,
    pub target_id: Uuid,
    pub target_username: String,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

// ─── Router ────────────────────────────────────────────────────────────────

pub fn trust_router() -> Router {
    Router::new()
        .route("/trust/endorse", post(endorse_handler))
        .route("/trust/revoke", post(revoke_handler))
        .route("/trust/denounce", post(denounce_handler))
        .route(
            "/trust/denouncements/mine",
            get(list_my_denouncements_handler),
        )
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

    if body.weight <= 0.0 || body.weight > 1.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "weight must be in range (0.0, 1.0]".to_string(),
            }),
        )
            .into_response();
    }

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

    if body.reason.is_empty() || body.reason.len() > 500 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "reason must be between 1 and 500 characters".to_string(),
            }),
        )
            .into_response();
    }

    match trust_service
        .denounce(auth.account_id, body.target_id, &body.reason)
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

async fn list_my_denouncements_handler(
    Extension(pool): Extension<PgPool>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, DenouncementWithUsername>(
        "SELECT d.id, d.target_id, a.username AS target_username, d.reason, d.created_at \
         FROM trust__denouncements d \
         JOIN accounts a ON a.id = d.target_id \
         WHERE d.accuser_id = $1 \
         ORDER BY d.created_at DESC",
    )
    .bind(auth.account_id)
    .fetch_all(&pool)
    .await;

    match result {
        Ok(rows) => {
            let denouncements = rows
                .into_iter()
                .map(|r| DenouncementResponse {
                    id: r.id,
                    target_id: r.target_id,
                    target_username: r.target_username,
                    reason: r.reason,
                    created_at: r.created_at.to_rfc3339(),
                })
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(denouncements)).into_response()
        }
        Err(e) => {
            tracing::error!("list_my_denouncements failed: {e}");
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

/// Demo endorsement slot count (k=3).
const ENDORSEMENT_SLOTS: u32 = 3;
/// Permanent denouncement budget per user (d=2, ADR-020).
const DENOUNCEMENT_SLOTS: u32 = 2;

async fn budget_handler(
    Extension(reputation_repo): Extension<Arc<dyn ReputationRepo>>,
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let endorsements_used = match reputation_repo
        .count_active_trust_endorsements_by(auth.account_id)
        .await
    {
        Ok(n) => n,
        Err(ref e) => {
            tracing::error!("Budget handler endorsement count error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response();
        }
    };

    let denouncements_used = match trust_repo
        .count_active_denouncements_by(auth.account_id)
        .await
    {
        Ok(n) => n,
        Err(ref e) => {
            tracing::error!("Budget handler denouncement count error: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(BudgetResponse {
            slots_total: ENDORSEMENT_SLOTS,
            slots_used: endorsements_used,
            slots_available: i64::from(ENDORSEMENT_SLOTS) - endorsements_used,
            denouncements_total: DENOUNCEMENT_SLOTS,
            denouncements_used,
            denouncements_available: i64::from(DENOUNCEMENT_SLOTS) - denouncements_used,
        }),
    )
        .into_response()
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

    // Use the client-supplied weight if present; otherwise compute from method + depth.
    let weight = body.weight.unwrap_or_else(|| {
        compute_endorsement_weight(&body.delivery_method, body.relationship_depth.as_deref())
    });
    if weight <= 0.0 || weight > 1.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "weight must be in range (0.0, 1.0]".to_string(),
            }),
        )
            .into_response();
    }

    let expires_at = Utc::now() + Duration::days(7);

    match trust_repo
        .create_invite(
            auth.account_id,
            &envelope_bytes,
            &body.delivery_method,
            body.relationship_depth.as_deref(),
            weight,
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
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
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

            // Auto-enqueue endorsement — the stored signed envelope is the endorser's authorization.
            // Use the weight captured at invite creation time (set by the endorser's method choice).
            if let Err(e) = trust_service
                .endorse(
                    invite.endorser_id,
                    auth.account_id,
                    invite.weight,
                    Some(invite.attestation.clone()),
                )
                .await
            {
                tracing::warn!(
                    "auto-endorse after invite accept failed for endorser={}: {e}",
                    invite.endorser_id
                );
            }

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
        TrustServiceError::EndorsementSlotsExhausted { max } => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorResponse {
                error: format!("Endorsement slots exhausted (max {max})"),
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
        TrustServiceError::DenouncementConflict => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "Cannot endorse a user you have denounced".to_string(),
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
        TrustServiceError::EndorsementRepo(ref inner) => {
            tracing::error!("Trust service endorsement repo error: {inner}");
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
