use anyhow::anyhow;
use axum::http::StatusCode;
use axum::{Extension, Json};
use base64::Engine;
use rand::random;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::identity::crypto::{canonicalize_value, derive_kid, verify_envelope, verify_signature};

use super::accounts::{decode_key, internal_error};

const CHALLENGE_TTL_SECONDS: i64 = 300;

#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    pub account_id: Uuid,
    pub device_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub nonce: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub challenge_id: Uuid,
    pub account_id: Uuid,
    pub device_id: Uuid,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub session_id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Issue a login challenge for a device. Stores nonce and expiry in sessions table.
///
/// # Errors
/// Returns a 4xx for validation/delegation failures or 500 on persistence errors.
pub async fn issue_challenge(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, (StatusCode, String)> {
    let account = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, root_kid, root_pubkey FROM accounts WHERE id = $1",
    )
    .bind(payload.account_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "account not found".to_string()))?;

    let root_pubkey_bytes = decode_key(&account.2, "root_pubkey")?;
    let expected_root_kid = derive_kid(&root_pubkey_bytes);
    if account.1 != expected_root_kid {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored root_kid does not match pubkey".to_string(),
        ));
    }

    let device = sqlx::query_as::<_, (String, String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT device_kid, device_pubkey, revoked_at FROM devices WHERE id = $1 AND account_id = $2",
    )
    .bind(payload.device_id)
    .bind(payload.account_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "device not found".to_string()))?;

    if device.2.is_some() {
        return Err((StatusCode::FORBIDDEN, "device revoked".to_string()));
    }

    let delegation = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT delegation_envelope FROM device_delegations WHERE account_id = $1 AND device_id = $2 AND revoked_at IS NULL",
    )
    .bind(payload.account_id)
    .bind(payload.device_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (
        StatusCode::FORBIDDEN,
        "no active delegation for device".to_string(),
    ))?;

    let envelope: crate::identity::crypto::SignedEnvelope =
        serde_json::from_value(delegation.0).map_err(|e| internal_error(anyhow!(e)))?;
    verify_envelope(&envelope, &root_pubkey_bytes)
        .map_err(|err| (StatusCode::FORBIDDEN, format!("invalid delegation: {err}")))?;

    let challenge_id = Uuid::new_v4();
    let nonce_bytes: [u8; 32] = random();
    let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce_bytes);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(CHALLENGE_TTL_SECONDS);

    sqlx::query(
        r#"
        INSERT INTO sessions (id, account_id, device_id, issued_at, expires_at, scopes, auth_factors, challenge_nonce, challenge_expires_at)
        VALUES ($1, $2, $3, NOW(), $4, '{}', '{"cryptographic":false}', $5, $4)
        "#,
    )
    .bind(challenge_id)
    .bind(payload.account_id)
    .bind(payload.device_id)
    .bind(expires_at)
    .bind(&nonce)
    .execute(&pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(ChallengeResponse {
        challenge_id,
        nonce,
        expires_at,
    }))
}

/// Verify a signed challenge and activate a session.
///
/// # Errors
/// Returns a 4xx for bad signatures/expired challenges or 500 on persistence errors.
#[allow(clippy::too_many_lines)]
pub async fn verify_challenge(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, String)> {
    let session = sqlx::query_as::<_, (Uuid, Uuid, Option<String>, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>, chrono::DateTime<chrono::Utc>)>(
        "SELECT account_id, device_id, challenge_nonce, challenge_expires_at, used_at, expires_at FROM sessions WHERE id = $1",
    )
    .bind(payload.challenge_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "challenge not found".to_string()))?;

    if session.0 != payload.account_id || session.1 != payload.device_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "account/device mismatch".to_string(),
        ));
    }

    if session.4.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "challenge already used".to_string(),
        ));
    }

    let expires_at = session.3.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "challenge missing expiry".to_string(),
        )
    })?;
    if expires_at < chrono::Utc::now() {
        return Err((StatusCode::BAD_REQUEST, "challenge expired".to_string()));
    }

    let nonce = session.2.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "challenge missing nonce".to_string(),
        )
    })?;

    let device = sqlx::query_as::<_, (String, String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT device_kid, device_pubkey, revoked_at FROM devices WHERE id = $1 AND account_id = $2",
    )
    .bind(payload.device_id)
    .bind(payload.account_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "device not found".to_string()))?;

    if device.2.is_some() {
        return Err((StatusCode::FORBIDDEN, "device revoked".to_string()));
    }

    let account = sqlx::query_as::<_, (String,)>("SELECT root_pubkey FROM accounts WHERE id = $1")
        .bind(payload.account_id)
        .fetch_one(&pool)
        .await
        .map_err(internal_error)?;

    let root_pubkey_bytes = decode_key(&account.0, "root_pubkey")?;
    let delegation = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT delegation_envelope FROM device_delegations WHERE account_id = $1 AND device_id = $2 AND revoked_at IS NULL",
    )
    .bind(payload.account_id)
    .bind(payload.device_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (
        StatusCode::FORBIDDEN,
        "no active delegation for device".to_string(),
    ))?;

    let envelope: crate::identity::crypto::SignedEnvelope =
        serde_json::from_value(delegation.0).map_err(|e| internal_error(anyhow!(e)))?;
    verify_envelope(&envelope, &root_pubkey_bytes)
        .map_err(|err| (StatusCode::FORBIDDEN, format!("invalid delegation: {err}")))?;

    let payload_json = json!({
        "challenge_id": payload.challenge_id,
        "nonce": nonce,
        "account_id": payload.account_id,
        "device_id": payload.device_id,
    });
    let canonical = canonicalize_value(&payload_json).map_err(|e| internal_error(anyhow!(e)))?;
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.signature.as_bytes())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "invalid signature encoding".to_string(),
            )
        })?;
    let device_pubkey_bytes = decode_key(&device.1, "device_pubkey")?;

    verify_signature(&canonical, &device_pubkey_bytes, &signature).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            "signature verification failed".to_string(),
        )
    })?;

    let expires_at = session.5;

    sqlx::query(
        "UPDATE sessions SET used_at = NOW(), auth_factors = '{\"cryptographic\":true}'::jsonb WHERE id = $1",
    )
    .bind(payload.challenge_id)
    .execute(&pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(VerifyResponse {
        session_id: payload.challenge_id,
        expires_at,
    }))
}
