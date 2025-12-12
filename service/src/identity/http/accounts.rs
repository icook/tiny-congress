use axum::extract::Extension;
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::identity::crypto::{derive_kid, verify_envelope, SignedEnvelope};
use crate::identity::repo::event_store::{append_signed_event, AppendEventInput};

#[derive(Debug, Deserialize)]
pub struct DeviceMetadata {
    pub name: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub root_pubkey: String,
    pub device_pubkey: String,
    pub device_metadata: Option<DeviceMetadata>,
    pub delegation_envelope: SignedEnvelope,
}

#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub account_id: Uuid,
    pub device_id: Uuid,
    pub root_kid: String,
}

pub async fn signup(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<SignupRequest>,
) -> Result<Json<SignupResponse>, (StatusCode, String)> {
    let username = payload.username.trim().to_lowercase();
    if username.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "username is required".to_string()));
    }

    // Decode keys
    let root_pubkey_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.root_pubkey.as_bytes())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "invalid root_pubkey encoding".to_string(),
            )
        })?;

    let device_pubkey_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.device_pubkey.as_bytes())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "invalid device_pubkey encoding".to_string(),
            )
        })?;

    let root_kid = derive_kid(&root_pubkey_bytes);

    // Basic envelope checks
    let device_id = extract_device_id(&payload.delegation_envelope)?;
    let signer_kid = payload.delegation_envelope.signer.kid.clone();
    if signer_kid != root_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "delegation signer kid does not match root pubkey".to_string(),
        ));
    }

    if let Err(err) = verify_envelope(&payload.delegation_envelope, &root_pubkey_bytes) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("invalid delegation signature: {err}"),
        ));
    }

    let account_id = Uuid::new_v4();

    // Append sigchain link
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id,
            seqno: 1,
            event_type: "AccountCreated".to_string(),
            envelope: payload.delegation_envelope.clone(),
            signer_pubkey: &root_pubkey_bytes,
        },
    )
    .await
    .map_err(internal_error)?;

    // Persist account + device rows
    let mut tx = pool
        .begin()
        .await
        .map_err(|err| internal_error(anyhow::anyhow!(err)))?;

    // enforce unique username
    let existing: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM accounts WHERE username = $1")
        .bind(&username)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal_error)?;

    if existing.is_some() {
        return Err((StatusCode::CONFLICT, "username already exists".to_string()));
    }

    sqlx::query(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(account_id)
    .bind(&username)
    .bind(&root_kid)
    .bind(&payload.root_pubkey)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    let device_kid = derive_kid(&device_pubkey_bytes);
    let device_type = payload
        .device_metadata
        .as_ref()
        .and_then(|m| m.r#type.clone())
        .unwrap_or_else(|| "other".to_string());

    sqlx::query(
        r#"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(device_id)
    .bind(account_id)
    .bind(&device_kid)
    .bind(&payload.device_pubkey)
    .bind(
        payload
            .device_metadata
            .as_ref()
            .and_then(|m| m.name.clone()),
    )
    .bind(device_type)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"
        INSERT INTO device_delegations (account_id, device_id, delegation_envelope)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(account_id)
    .bind(device_id)
    .bind(serde_json::to_value(&payload.delegation_envelope).map_err(internal_error)?)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(SignupResponse {
        account_id,
        device_id,
        root_kid,
    }))
}

fn extract_device_id(envelope: &SignedEnvelope) -> Result<Uuid, (StatusCode, String)> {
    let value = envelope.payload.get("device_id").ok_or((
        StatusCode::BAD_REQUEST,
        "device_id missing in payload".to_string(),
    ))?;
    match value {
        serde_json::Value::String(s) => Uuid::parse_str(s).map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "device_id must be a UUID".to_string(),
            )
        }),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "device_id must be a string".to_string(),
        )),
    }
}

fn internal_error<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
