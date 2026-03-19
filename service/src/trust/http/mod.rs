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
use super::weight::compute_endorsement_weight;
use crate::http::{bad_request, conflict, internal_error, not_found, too_many_requests};
use crate::identity::http::auth::AuthenticatedDevice;
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
    pub out_of_slot_count: i64,
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

    if !body.weight.is_finite() || body.weight <= 0.0 || body.weight > 1.0 {
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
        return bad_request("reason must be between 1 and 500 characters");
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
/// Valid delivery methods for invites (must match migration 18 CHECK constraint).
const VALID_DELIVERY_METHODS: &[&str] = &["qr", "email", "video", "text", "messaging"];
/// Valid relationship depths for invites (must match migration 18 CHECK constraint).
const VALID_RELATIONSHIP_DEPTHS: &[&str] = &["years", "months", "acquaintance"];

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
        .count_active_denouncements_by(auth.account_id)
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
            slots_total: ENDORSEMENT_SLOTS,
            slots_used: endorsements_used,
            slots_available: i64::from(ENDORSEMENT_SLOTS) - endorsements_used,
            out_of_slot_count,
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
        return bad_request("Invalid base64url encoding for envelope");
    };

    if !VALID_DELIVERY_METHODS.contains(&body.delivery_method.as_str()) {
        return bad_request("delivery_method must be one of: qr, email, video, text, messaging");
    }

    if let Some(ref depth) = body.relationship_depth {
        if !VALID_RELATIONSHIP_DEPTHS.contains(&depth.as_str()) {
            return bad_request("relationship_depth must be one of: years, months, acquaintance");
        }
    }

    // Use the client-supplied weight if present; otherwise compute from method + depth.
    let weight = body.weight.unwrap_or_else(|| {
        compute_endorsement_weight(&body.delivery_method, body.relationship_depth.as_deref())
    });
    if !weight.is_finite() || weight <= 0.0 || weight > 1.0 {
        return bad_request("weight must be in range (0.0, 1.0]");
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
        TrustServiceError::SelfAction => bad_request("Cannot target yourself"),
        TrustServiceError::QuotaExceeded => too_many_requests("Daily action quota exceeded"),
        TrustServiceError::EndorsementSlotsExhausted { max } => {
            too_many_requests(&format!("Endorsement slots exhausted (max {max})"))
        }
        TrustServiceError::DenouncementSlotsExhausted { max } => {
            too_many_requests(&format!("Denouncement slots exhausted (max {max})"))
        }
        TrustServiceError::DenouncementConflict => {
            conflict("Cannot endorse a user you have denounced")
        }
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
