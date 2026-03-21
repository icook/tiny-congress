// lint-patterns:allow-no-utoipa — tracked by #861 (PR #905)
//! HTTP handlers for trust system — endorsements, denouncements, invites, scores, budget.

use std::sync::Arc;

use axum::{
    extract::Extension,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::repo::{TrustRepo, TrustRepoError};
use super::service::{
    is_valid_endorsement_weight, is_valid_reason, TrustService, TrustServiceError,
    DENOUNCEMENT_SLOT_LIMIT, ENDORSEMENT_SLOT_LIMIT,
};
use super::weight::{compute_endorsement_weight, DeliveryMethod, RelationshipDepth};
use crate::http::{bad_request, conflict, internal_error, not_found, too_many_requests, Path};
use crate::identity::http::auth::AuthenticatedDevice;
use crate::reputation::repo::ReputationRepo;

// ─── Request types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct EndorseRequest {
    #[schema(value_type = String, format = "uuid")]
    pub subject_id: Uuid,
    #[serde(default = "default_weight")]
    pub weight: f32,
    pub attestation: Option<serde_json::Value>,
}

const fn default_weight() -> f32 {
    1.0
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RevokeRequest {
    #[schema(value_type = String, format = "uuid")]
    pub subject_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DenounceRequest {
    #[schema(value_type = String, format = "uuid")]
    pub target_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInviteRequest {
    /// base64url-encoded invite envelope bytes
    pub envelope: String,
    #[schema(value_type = String)]
    pub delivery_method: DeliveryMethod,
    #[schema(value_type = Option<String>)]
    pub relationship_depth: Option<RelationshipDepth>,
    pub weight: Option<f32>,
    pub attestation: serde_json::Value,
}

// ─── Response types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ScoreSnapshotResponse {
    #[schema(value_type = Option<String>, format = "uuid")]
    pub context_user_id: Option<Uuid>,
    pub trust_distance: Option<f32>,
    pub path_diversity: Option<i32>,
    pub eigenvector_centrality: Option<f32>,
    pub computed_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ScoresResponse {
    pub scores: Vec<ScoreSnapshotResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetResponse {
    pub slots_total: u32,
    pub slots_used: i64,
    pub slots_available: i64,
    pub out_of_slot_count: i64,
    pub denouncements_total: u32,
    pub denouncements_used: i64,
    pub denouncements_available: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateInviteResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub expires_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InviteResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub delivery_method: String,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub accepted_by: Option<Uuid>,
    pub expires_at: String,
    pub accepted_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitesResponse {
    pub invites: Vec<InviteResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AcceptInviteResponse {
    #[schema(value_type = String, format = "uuid")]
    pub endorser_id: Uuid,
    pub accepted_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DenouncementResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub target_id: Uuid,
    pub target_username: String,
    pub reason: String,
    pub created_at: String,
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

#[utoipa::path(
    post,
    path = "/trust/endorse",
    tag = "Trust",
    request_body = EndorseRequest,
    responses(
        (status = 202, description = "Endorsement queued", body = MessageResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 429, description = "Quota exceeded"),
    )
)]
async fn endorse_handler(
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: EndorseRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    if !is_valid_endorsement_weight(body.weight) {
        return bad_request("weight must be in range (0.0, 1.0]");
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
        Ok(()) => {
            tracing::info!(
                actor_id = %auth.account_id,
                subject_id = %body.subject_id,
                "Endorsement queued"
            );
            (
                StatusCode::ACCEPTED,
                Json(MessageResponse {
                    message: "endorsement queued".to_string(),
                }),
            )
                .into_response()
        }
        Err(ref e) => trust_service_error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/trust/revoke",
    tag = "Trust",
    request_body = RevokeRequest,
    responses(
        (status = 202, description = "Revocation queued", body = MessageResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 429, description = "Quota exceeded"),
    )
)]
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
        Ok(()) => {
            tracing::info!(
                actor_id = %auth.account_id,
                subject_id = %body.subject_id,
                "Revocation queued"
            );
            (
                StatusCode::ACCEPTED,
                Json(MessageResponse {
                    message: "revocation queued".to_string(),
                }),
            )
                .into_response()
        }
        Err(ref e) => trust_service_error_response(e),
    }
}

#[utoipa::path(
    post,
    path = "/trust/denounce",
    tag = "Trust",
    request_body = DenounceRequest,
    responses(
        (status = 202, description = "Denouncement queued", body = MessageResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 429, description = "Quota exceeded"),
    )
)]
async fn denounce_handler(
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: DenounceRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    if !is_valid_reason(&body.reason) {
        return bad_request("reason must be between 1 and 500 characters");
    }

    match trust_service
        .denounce(auth.account_id, body.target_id, &body.reason)
        .await
    {
        Ok(()) => {
            tracing::info!(
                actor_id = %auth.account_id,
                target_id = %body.target_id,
                "Denouncement queued"
            );
            (
                StatusCode::ACCEPTED,
                Json(MessageResponse {
                    message: "denouncement queued".to_string(),
                }),
            )
                .into_response()
        }
        Err(ref e) => trust_service_error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/trust/denouncements/mine",
    tag = "Trust",
    responses(
        (status = 200, description = "List of denouncements", body = Vec<DenouncementResponse>),
        (status = 401, description = "Unauthorized"),
    )
)]
async fn list_my_denouncements_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match trust_repo
        .list_denouncements_by_with_username(auth.account_id)
        .await
    {
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
        Err(ref e) => trust_repo_error_response(e),
    }
}

#[utoipa::path(
    get,
    path = "/trust/scores/me",
    tag = "Trust",
    responses(
        (status = 200, description = "Trust scores for the authenticated user", body = ScoresResponse),
        (status = 401, description = "Unauthorized"),
    )
)]
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

#[utoipa::path(
    get,
    path = "/trust/budget",
    tag = "Trust",
    responses(
        (status = 200, description = "Endorsement and denouncement budget for the authenticated user", body = BudgetResponse),
        (status = 401, description = "Unauthorized"),
    )
)]
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
            return internal_error();
        }
    };

    let all_endorsements = match reputation_repo
        .count_all_active_trust_endorsements_by(auth.account_id)
        .await
    {
        Ok(n) => n,
        Err(ref e) => {
            tracing::error!("Budget handler all-endorsement count error: {e}");
            return internal_error();
        }
    };
    let out_of_slot_count = all_endorsements - endorsements_used;

    let denouncements_used = match trust_repo
        .count_total_denouncements_by(auth.account_id)
        .await
    {
        Ok(n) => n,
        Err(ref e) => {
            tracing::error!("Budget handler denouncement count error: {e}");
            return internal_error();
        }
    };

    (
        StatusCode::OK,
        Json(BudgetResponse {
            slots_total: ENDORSEMENT_SLOT_LIMIT,
            slots_used: endorsements_used,
            slots_available: i64::from(ENDORSEMENT_SLOT_LIMIT) - endorsements_used,
            out_of_slot_count,
            denouncements_total: DENOUNCEMENT_SLOT_LIMIT,
            denouncements_used,
            denouncements_available: i64::from(DENOUNCEMENT_SLOT_LIMIT) - denouncements_used,
        }),
    )
        .into_response()
}

#[utoipa::path(
    post,
    path = "/trust/invites",
    tag = "Trust",
    request_body = CreateInviteRequest,
    responses(
        (status = 201, description = "Invite created", body = CreateInviteResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
    )
)]
async fn create_invite_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let body: CreateInviteRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };

    let Ok(envelope_bytes) = tc_crypto::decode_base64url(&body.envelope) else {
        return bad_request("Invalid base64url encoding for envelope");
    };

    if envelope_bytes.is_empty() || envelope_bytes.len() > 4096 {
        return bad_request("envelope must be between 1 and 4096 bytes");
    }

    // Use the client-supplied weight if present; otherwise compute from method + depth.
    let weight = body.weight.unwrap_or_else(|| {
        compute_endorsement_weight(body.delivery_method, body.relationship_depth)
    });
    if !is_valid_endorsement_weight(weight) {
        return bad_request("weight must be in range (0.0, 1.0]");
    }

    let expires_at = Utc::now() + Duration::days(7);

    match trust_repo
        .create_invite(
            auth.account_id,
            &envelope_bytes,
            body.delivery_method,
            body.relationship_depth,
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

#[utoipa::path(
    get,
    path = "/trust/invites/mine",
    tag = "Trust",
    responses(
        (status = 200, description = "List of invites created by the authenticated user", body = InvitesResponse),
        (status = 401, description = "Unauthorized"),
    )
)]
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

#[utoipa::path(
    post,
    path = "/trust/invites/{id}/accept",
    tag = "Trust",
    params(
        ("id" = String, Path, description = "Invite ID", format = "uuid")
    ),
    responses(
        (status = 200, description = "Invite accepted and endorsement queued", body = AcceptInviteResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Invite not found"),
    )
)]
async fn accept_invite_handler(
    Extension(trust_repo): Extension<Arc<dyn TrustRepo>>,
    Extension(trust_service): Extension<Arc<dyn TrustService>>,
    Path(invite_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    // Reject self-accept before touching state: endorser_id is immutable so this
    // check is race-free even though accept_invite runs as a separate SQL statement.
    match trust_repo.get_invite(invite_id).await {
        Ok(invite) if invite.endorser_id == auth.account_id => {
            return bad_request("Cannot accept your own invite");
        }
        Ok(_) => {}
        Err(TrustRepoError::NotFound) => return not_found("Invite not found"),
        Err(ref e) => return trust_repo_error_response(e),
    }

    match trust_repo.accept_invite(invite_id, auth.account_id).await {
        Ok(invite) => {
            let accepted_at = match invite.accepted_at {
                Some(t) => t.to_rfc3339(),
                None => {
                    return internal_error();
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
        Err(TrustRepoError::NotFound) => not_found("Invite not found"),
        Err(ref e) => trust_repo_error_response(e),
    }
}

// ─── Error mapping ─────────────────────────────────────────────────────────

fn trust_service_error_response(e: &TrustServiceError) -> axum::response::Response {
    match e {
        TrustServiceError::InvalidWeight => bad_request("weight must be in range (0.0, 1.0]"),
        TrustServiceError::InvalidReason { max } => {
            bad_request(&format!("reason must be between 1 and {max} characters"))
        }
        TrustServiceError::SelfAction => bad_request("Cannot target yourself"),
        TrustServiceError::QuotaExceeded => too_many_requests("Daily action quota exceeded"),
        TrustServiceError::DenouncementSlotsExhausted { max } => {
            too_many_requests(&format!("Denouncement slots exhausted (max {max})"))
        }
        TrustServiceError::DenouncementConflict => {
            conflict("Cannot endorse a user you have denounced")
        }
        TrustServiceError::AlreadyDenounced => conflict("Already denounced this user"),
        TrustServiceError::Repo(ref inner) => {
            tracing::error!("Trust service repo error: {inner}");
            internal_error()
        }
        TrustServiceError::EndorsementRepo(ref inner) => {
            tracing::error!("Trust service endorsement repo error: {inner}");
            internal_error()
        }
    }
}

fn trust_repo_error_response(e: &TrustRepoError) -> axum::response::Response {
    match e {
        TrustRepoError::NotFound => not_found("Not found"),
        TrustRepoError::Duplicate => conflict("Duplicate entry"),
        TrustRepoError::Database(ref inner) => {
            tracing::error!("Trust repo database error: {inner}");
            internal_error()
        }
    }
}
