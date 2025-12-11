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

#[derive(Debug)]
struct PreparedSignup {
    username: String,
    root_pubkey_b64: String,
    root_pubkey_bytes: Vec<u8>,
    root_kid: String,
    device_pubkey_b64: String,
    device_kid: String,
    device_name: Option<String>,
    device_type: String,
    device_id: Uuid,
    delegation_envelope: SignedEnvelope,
}

/// Register a new account with a root key, first device, and delegation link.
///
/// # Errors
/// Returns a 400 for validation/signature failures or a 500 when persistence fails.
pub async fn signup(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<SignupRequest>,
) -> Result<Json<SignupResponse>, (StatusCode, String)> {
    let prepared = prepare_signup_request(payload)?;
    let delegation_envelope = prepared.delegation_envelope.clone();
    let account_id = Uuid::new_v4();

    // Append sigchain link
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id,
            seqno: 1,
            event_type: "AccountCreated".to_string(),
            envelope: prepared.delegation_envelope.clone(),
            signer_pubkey: &prepared.root_pubkey_bytes,
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
        .bind(&prepared.username)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal_error)?;

    if existing.is_some() {
        return Err((StatusCode::CONFLICT, "username already exists".to_string()));
    }

    sqlx::query(
        r"
        INSERT INTO accounts (id, username, root_kid, root_pubkey)
        VALUES ($1, $2, $3, $4)
        ",
    )
    .bind(account_id)
    .bind(&prepared.username)
    .bind(&prepared.root_kid)
    .bind(&prepared.root_pubkey_b64)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(prepared.device_id)
    .bind(account_id)
    .bind(&prepared.device_kid)
    .bind(&prepared.device_pubkey_b64)
    .bind(prepared.device_name)
    .bind(prepared.device_type)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r"
        INSERT INTO device_delegations (account_id, device_id, delegation_envelope)
        VALUES ($1, $2, $3)
        ",
    )
    .bind(account_id)
    .bind(prepared.device_id)
    .bind(serde_json::to_value(&delegation_envelope).map_err(internal_error)?)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(SignupResponse {
        account_id,
        device_id: prepared.device_id,
        root_kid: prepared.root_kid,
    }))
}

pub(crate) fn extract_device_id(envelope: &SignedEnvelope) -> Result<Uuid, (StatusCode, String)> {
    let value = envelope.payload.get("device_id").ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "device_id missing in payload".to_string(),
        )
    })?;
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

pub(crate) fn internal_error<E: std::fmt::Display>(err: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

pub(crate) fn decode_key(encoded: &str, field: &str) -> Result<Vec<u8>, (StatusCode, String)> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|_| (StatusCode::BAD_REQUEST, format!("invalid {field} encoding")))
}

fn prepare_signup_request(payload: SignupRequest) -> Result<PreparedSignup, (StatusCode, String)> {
    let username = payload.username.trim().to_lowercase();
    if username.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "username is required".to_string()));
    }

    let root_pubkey_bytes = decode_key(&payload.root_pubkey, "root_pubkey")?;
    let root_kid = derive_kid(&root_pubkey_bytes);

    // Basic envelope checks
    let device_id = extract_device_id(&payload.delegation_envelope)?;
    if payload.delegation_envelope.signer.kid != root_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "delegation signer kid does not match root pubkey".to_string(),
        ));
    }

    verify_envelope(&payload.delegation_envelope, &root_pubkey_bytes).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid delegation signature: {err}"),
        )
    })?;

    let device_pubkey_bytes = decode_key(&payload.device_pubkey, "device_pubkey")?;
    let device_kid = derive_kid(&device_pubkey_bytes);

    let (device_name, device_type) = payload.device_metadata.map_or_else(
        || (None, "other".to_string()),
        |meta| {
            let name = meta.name;
            let device_type = meta.r#type.unwrap_or_else(|| "other".to_string());
            (name, device_type)
        },
    );

    Ok(PreparedSignup {
        username,
        root_pubkey_b64: payload.root_pubkey,
        root_pubkey_bytes,
        root_kid,
        device_pubkey_b64: payload.device_pubkey,
        device_kid,
        device_name,
        device_type,
        device_id,
        delegation_envelope: payload.delegation_envelope,
    })
}
