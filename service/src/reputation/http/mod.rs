//! HTTP handlers for reputation system

pub mod idme;

use std::sync::Arc;

use axum::{
    extract::{Extension, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::service::{EndorsementError, EndorsementService};
use crate::config::RateLimitConfig;
use crate::http::rate_limit::make_governor_layer;
use crate::http::ErrorResponse;
use crate::identity::http::auth::AuthenticatedDevice;
use crate::identity::repo::{AccountRepoError, IdentityRepo};

// ─── Response types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EndorsementResponse {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub topic: String,
    pub issuer_id: Option<Uuid>,
    pub created_at: String,
    pub revoked: bool,
}

#[derive(Debug, Serialize)]
pub struct EndorsementsListResponse {
    pub endorsements: Vec<EndorsementResponse>,
}

#[derive(Debug, Serialize)]
pub struct HasEndorsementResponse {
    pub has_endorsement: bool,
}

// ─── Verifier endpoint types ──────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEndorsementRequest {
    pub username: String,
    pub topic: String,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedEndorsementResponse {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub topic: String,
    pub issuer_id: Uuid,
    pub created_at: String,
}

// ─── Query types ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EndorsementQuery {
    pub subject_id: Option<Uuid>,
    pub topic: Option<String>,
}

// ─── Router ────────────────────────────────────────────────────────────────

pub fn router(rate_limit_config: &RateLimitConfig) -> Router {
    // ID.me OAuth endpoints are unauthenticated — apply the same limit as
    // other auth flows (backup_per_minute reused as the "generic auth" limit).
    let idme_router = {
        let r = Router::new()
            .route("/auth/idme/authorize", get(idme::authorize))
            .route("/auth/idme/callback", get(idme::callback));
        if let Some(layer) =
            make_governor_layer(rate_limit_config.backup_per_minute, rate_limit_config)
        {
            r.layer(layer)
        } else {
            r
        }
    };

    Router::new()
        .route("/me/endorsements", get(my_endorsements))
        .route("/endorsements/check", get(check_endorsement))
        .route(
            "/verifiers/endorsements",
            post(create_endorsement_as_verifier),
        )
        .merge(idme_router)
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// List endorsements for the authenticated user.
async fn my_endorsements(
    Extension(service): Extension<Arc<dyn EndorsementService>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match service.list_endorsements(auth.account_id).await {
        Ok(endorsements) => {
            let response = EndorsementsListResponse {
                endorsements: endorsements
                    .into_iter()
                    .map(|e| EndorsementResponse {
                        id: e.id,
                        subject_id: e.subject_id,
                        topic: e.topic,
                        issuer_id: e.endorser_id,
                        created_at: e.created_at.to_rfc3339(),
                        revoked: e.revoked_at.is_some(),
                    })
                    .collect(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => endorsement_error_response(e),
    }
}

/// Check if a subject has an endorsement for a topic (public endpoint).
async fn check_endorsement(
    Extension(service): Extension<Arc<dyn EndorsementService>>,
    Query(query): Query<EndorsementQuery>,
) -> impl IntoResponse {
    let Some(subject_id) = query.subject_id else {
        return crate::http::bad_request("subject_id query parameter is required");
    };

    let Some(ref topic) = query.topic else {
        return crate::http::bad_request("topic query parameter is required");
    };

    match service.has_endorsement(subject_id, topic).await {
        Ok(has) => (
            StatusCode::OK,
            Json(HasEndorsementResponse {
                has_endorsement: has,
            }),
        )
            .into_response(),
        Err(e) => endorsement_error_response(e),
    }
}

// ─── Verifier endpoint ────────────────────────────────────────────────────

/// Create an endorsement as an authorized verifier.
///
/// Checks that the authenticated caller has the `authorized_verifier` endorsement,
/// resolves the target username to an account, and creates the endorsement.
///
/// # Errors
///
/// Returns an error response for unauthorized, forbidden, not-found, or internal errors.
#[utoipa::path(
    post,
    path = "/verifiers/endorsements",
    tag = "Verifiers",
    request_body = CreateEndorsementRequest,
    responses(
        (status = 201, description = "Endorsement created", body = CreatedEndorsementResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Not an authorized verifier"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_endorsement_as_verifier(
    Extension(endorsement_service): Extension<Arc<dyn EndorsementService>>,
    Extension(identity_repo): Extension<Arc<dyn IdentityRepo>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    // Parse body from AuthenticatedDevice (which already consumed it for signing)
    let body: CreateEndorsementRequest = match auth.json() {
        Ok(b) => b,
        Err(e) => return e,
    };
    // 1. Check caller is an authorized verifier
    let is_verifier = match endorsement_service
        .has_endorsement(auth.account_id, "authorized_verifier")
        .await
    {
        Ok(has) => has,
        Err(e) => return endorsement_error_response(e),
    };
    if !is_verifier {
        return crate::http::forbidden("Account is not an authorized verifier");
    }

    // 2. Resolve username → account_id
    let subject = match identity_repo.get_account_by_username(&body.username).await {
        Ok(account) => account,
        Err(AccountRepoError::NotFound) => {
            return crate::http::not_found("User not found");
        }
        Err(e) => {
            tracing::error!("Account lookup failed: {e}");
            return crate::http::internal_error();
        }
    };

    // 3. Create endorsement
    match endorsement_service
        .create_endorsement(
            subject.id,
            &body.topic,
            Some(auth.account_id),
            body.evidence.as_ref(),
        )
        .await
    {
        Ok(created) => (
            StatusCode::CREATED,
            Json(CreatedEndorsementResponse {
                id: created.id,
                subject_id: created.subject_id,
                topic: created.topic,
                issuer_id: auth.account_id,
                created_at: chrono::Utc::now().to_rfc3339(),
            }),
        )
            .into_response(),
        Err(e) => endorsement_error_response(e),
    }
}

// ─── Error mapping ─────────────────────────────────────────────────────────

fn endorsement_error_response(e: EndorsementError) -> axum::response::Response {
    match e {
        EndorsementError::Validation(msg) => {
            (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })).into_response()
        }
        EndorsementError::Internal(ref msg) => {
            tracing::error!("Endorsement error: {msg}");
            crate::http::internal_error()
        }
    }
}
