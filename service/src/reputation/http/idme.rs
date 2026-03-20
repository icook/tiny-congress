// lint-patterns:allow-no-utoipa — OAuth redirects, not JSON API endpoints
//! ID.me OAuth 2.0 verification flow
//!
//! Handles the authorize redirect and callback for ID.me identity verification.
//! On successful verification, creates an endorsement for the authenticated user
//! and links the external identity for sybil prevention.

use std::sync::{Arc, OnceLock};

use axum::{
    extract::{Extension, Query},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::config::IdMeConfig;
use crate::identity::http::auth::AuthenticatedDevice;
use crate::reputation::repo::ReputationRepo;
use crate::reputation::service::EndorsementService;

type HmacSha256 = Hmac<Sha256>;

/// The account ID of the bootstrapped ID.me verifier, injected as an Axum extension.
#[derive(Clone)]
pub struct IdMeVerifierAccountId(pub Uuid);

#[allow(clippy::expect_used)] // builder().build() with no custom TLS config is infallible
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client")
    })
}

// ─── State parameter (anti-CSRF) ──────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct OAuthState {
    account_id: Uuid,
    nonce: String,
    ts: i64,
}

const STATE_MAX_AGE_SECS: i64 = 300;

fn sign_state(state: &OAuthState, secret: &[u8]) -> Result<String, &'static str> {
    let payload = serde_json::to_string(state).map_err(|_| "failed to serialize state")?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| "invalid HMAC secret")?;
    mac.update(payload.as_bytes());
    let sig = tc_crypto::encode_base64url(&mac.finalize().into_bytes());
    let payload_b64 = tc_crypto::encode_base64url(payload.as_bytes());
    Ok(format!("{payload_b64}.{sig}"))
}

fn verify_state(state_str: &str, secret: &[u8]) -> Result<OAuthState, &'static str> {
    let parts: Vec<&str> = state_str.splitn(2, '.').collect();
    if parts.len() != 2 {
        return Err("invalid state format");
    }

    let payload_bytes =
        tc_crypto::decode_base64url(parts[0]).map_err(|_| "invalid state encoding")?;
    let payload_str = std::str::from_utf8(&payload_bytes).map_err(|_| "invalid state encoding")?;

    let provided_sig =
        tc_crypto::decode_base64url(parts[1]).map_err(|_| "invalid state encoding")?;

    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| "invalid secret")?;
    mac.update(payload_str.as_bytes());
    mac.verify_slice(&provided_sig)
        .map_err(|_| "invalid state signature")?;

    let state: OAuthState =
        serde_json::from_str(payload_str).map_err(|_| "invalid state payload")?;

    let now = chrono::Utc::now().timestamp();
    let age = now - state.ts;
    if !(0..=STATE_MAX_AGE_SECS).contains(&age) {
        return Err("state expired");
    }

    Ok(state)
}

// ─── ID.me API types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    sub: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorizeResponse {
    url: String,
}

/// OAuth callback query parameters from ID.me.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// Generate the ID.me authorization URL and return it.
#[utoipa::path(
    get,
    path = "/auth/idme/authorize",
    tag = "reputation",
    responses(
        (status = 200, description = "Authorization URL generated", body = AuthorizeResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("device_auth" = []))
)]
pub async fn authorize(
    Extension(config): Extension<Arc<IdMeConfig>>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let nonce = tc_crypto::encode_base64url(&rand::random::<[u8; 16]>());
    let state = OAuthState {
        account_id: auth.account_id,
        nonce,
        ts: chrono::Utc::now().timestamp(),
    };
    let signed_state = match sign_state(&state, config.state_secret.as_bytes()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to sign OAuth state: {e}");
            return crate::http::internal_error();
        }
    };

    let authorize_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=openid&state={}",
        config.authorize_url,
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&config.redirect_uri),
        urlencoding::encode(&signed_state),
    );

    (
        StatusCode::OK,
        Json(AuthorizeResponse { url: authorize_url }),
    )
        .into_response()
}

/// OAuth callback from ID.me (browser redirect, unauthenticated).
///
/// The `account_id` is embedded in the HMAC-signed state parameter.
/// On success, redirects to the frontend with `verification=success`.
/// On failure, redirects with `verification=error&message=...`.
#[utoipa::path(
    get,
    path = "/auth/idme/callback",
    tag = "reputation",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from ID.me"),
        ("state" = Option<String>, Query, description = "HMAC-signed state parameter"),
        ("error" = Option<String>, Query, description = "Error code from ID.me"),
        ("error_description" = Option<String>, Query, description = "Human-readable error description")
    ),
    responses(
        (status = 302, description = "Redirect to frontend with verification result"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn callback(
    Extension(config): Extension<Arc<IdMeConfig>>,
    Extension(endorsement_service): Extension<Arc<dyn EndorsementService>>,
    Extension(repo): Extension<Arc<dyn ReputationRepo>>,
    Extension(verifier_id): Extension<IdMeVerifierAccountId>,
    Query(query): Query<CallbackQuery>,
) -> impl IntoResponse {
    let frontend_url = &config.frontend_callback_url;

    match process_callback(
        &config,
        &*endorsement_service,
        &*repo,
        verifier_id.0,
        &query,
    )
    .await
    {
        Ok(()) => redirect_to_frontend(frontend_url, "success", ""),
        Err(msg) => redirect_to_frontend(frontend_url, "error", &msg),
    }
}

// ─── Callback processing (extracted for line-count) ───────────────────────

async fn process_callback(
    config: &IdMeConfig,
    endorsement_service: &dyn EndorsementService,
    repo: &dyn ReputationRepo,
    verifier_account_id: Uuid,
    query: &CallbackQuery,
) -> Result<(), String> {
    // Handle errors from ID.me
    if let Some(ref error) = query.error {
        let desc = query
            .error_description
            .as_deref()
            .unwrap_or("Unknown error");
        tracing::warn!(error = %error, description = %desc, "ID.me authorization denied");
        return Err("Identity verification was denied or cancelled".to_string());
    }

    let code = query.code.as_deref().ok_or("Missing authorization code")?;
    let state_str = query.state.as_deref().ok_or("Missing state parameter")?;

    let state = verify_state(state_str, config.state_secret.as_bytes()).map_err(|e| {
        tracing::warn!(error = %e, "Invalid OAuth state");
        e.to_string()
    })?;

    let access_token = exchange_code(config, code).await?;
    let userinfo = fetch_userinfo(config, &access_token).await?;

    // Sybil check + link
    link_identity_if_new(repo, state.account_id, &userinfo.sub).await?;

    // Create endorsement
    create_verification_endorsement(
        endorsement_service,
        state.account_id,
        verifier_account_id,
        &userinfo.sub,
    )
    .await
}

async fn exchange_code(config: &IdMeConfig, code: &str) -> Result<String, String> {
    let resp = http_client()
        .post(&config.token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
        ])
        .send()
        .await
        .map_err(|e| {
            tracing::error!("ID.me token request failed: {e}");
            "Verification failed".to_string()
        })?;

    if !resp.status().is_success() {
        tracing::error!(status = %resp.status(), "ID.me token exchange failed");
        return Err("Verification failed".to_string());
    }

    let token: TokenResponse = resp.json().await.map_err(|e| {
        tracing::error!("Failed to parse ID.me token response: {e}");
        "Verification failed".to_string()
    })?;

    Ok(token.access_token)
}

async fn fetch_userinfo(
    config: &IdMeConfig,
    access_token: &str,
) -> Result<UserInfoResponse, String> {
    let resp = http_client()
        .get(&config.userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("ID.me userinfo request failed: {e}");
            "Verification failed".to_string()
        })?;

    if !resp.status().is_success() {
        tracing::error!(status = %resp.status(), "ID.me userinfo request failed");
        return Err("Verification failed".to_string());
    }

    resp.json().await.map_err(|e| {
        tracing::error!("Failed to parse ID.me userinfo response: {e}");
        "Verification failed".to_string()
    })
}

async fn link_identity_if_new(
    repo: &dyn ReputationRepo,
    account_id: Uuid,
    idme_sub: &str,
) -> Result<(), String> {
    match repo
        .get_external_identity_by_provider("idme", idme_sub)
        .await
    {
        Ok(existing) => {
            if existing.account_id != account_id {
                tracing::warn!(
                    idme_sub = %idme_sub,
                    existing_account = %existing.account_id,
                    requesting_account = %account_id,
                    "Sybil attempt: ID.me identity already linked to different account"
                );
                return Err("This identity is already linked to another account".to_string());
            }
            Ok(()) // Same account re-verifying
        }
        Err(crate::reputation::repo::ExternalIdentityRepoError::NotFound) => repo
            .link_external_identity(account_id, "idme", idme_sub)
            .await
            .map(|_| ())
            .map_err(|e| {
                tracing::error!("Failed to link external identity: {e}");
                "Verification failed".to_string()
            }),
        Err(e) => {
            tracing::error!("External identity lookup failed: {e}");
            Err("Verification failed".to_string())
        }
    }
}

async fn create_verification_endorsement(
    service: &dyn EndorsementService,
    account_id: Uuid,
    verifier_account_id: Uuid,
    idme_sub: &str,
) -> Result<(), String> {
    match service
        .create_endorsement(
            account_id,
            "identity_verified",
            Some(verifier_account_id),
            None,
        )
        .await
    {
        Ok(_) => {
            tracing::info!(account_id = %account_id, idme_sub = %idme_sub, "ID.me verification successful");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to create endorsement: {e}");
            Err("Verification failed".to_string())
        }
    }
}

fn redirect_to_frontend(base_url: &str, status: &str, message: &str) -> axum::response::Response {
    let sep = if base_url.contains('?') { '&' } else { '?' };
    let url = format!(
        "{base_url}{sep}verification={}&message={}",
        urlencoding::encode(status),
        urlencoding::encode(message),
    );
    Redirect::temporary(&url).into_response()
}
