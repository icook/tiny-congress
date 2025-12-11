use axum::extract::Extension;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::identity::crypto::{derive_kid, verify_envelope, SignedEnvelope};
use crate::identity::repo::event_store::{append_signed_event, AppendEventInput};

use super::accounts::{decode_key, extract_device_id, internal_error, DeviceMetadata};

#[derive(Debug, Deserialize)]
pub struct AddDeviceRequest {
    pub account_id: Uuid,
    pub device_pubkey: String,
    pub device_metadata: Option<DeviceMetadata>,
    pub delegation_envelope: SignedEnvelope,
}

#[derive(Debug, Serialize)]
pub struct AddDeviceResponse {
    pub device_id: Uuid,
    pub device_kid: String,
}

#[derive(Debug)]
struct PreparedDeviceAdd {
    account_id: Uuid,
    root_pubkey_bytes: Vec<u8>,
    device_pubkey_b64: String,
    device_kid: String,
    device_id: Uuid,
    device_name: Option<String>,
    device_type: String,
    delegation_envelope: SignedEnvelope,
    next_seqno: i64,
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    root_kid: String,
    root_pubkey: String,
}

/// Append a root-signed delegation for a new device and persist it.
///
/// # Errors
/// Returns 400 for validation/signature issues, 404 when the account is missing, 409 on conflicts, and 500 on persistence failures.
pub async fn add_device(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<AddDeviceRequest>,
) -> Result<Json<AddDeviceResponse>, (StatusCode, String)> {
    let prepared = prepare_device_add(&pool, payload).await?;

    // Append delegation to sigchain
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id: prepared.account_id,
            seqno: prepared.next_seqno,
            event_type: "DeviceDelegation".to_string(),
            envelope: prepared.delegation_envelope.clone(),
            signer_pubkey: &prepared.root_pubkey_bytes,
        },
    )
    .await
    .map_err(internal_error)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|err| internal_error(anyhow::anyhow!(err)))?;

    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM devices WHERE account_id = $1 AND (id = $2 OR device_kid = $3)",
    )
    .bind(prepared.account_id)
    .bind(prepared.device_id)
    .bind(&prepared.device_kid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if exists.is_some() {
        return Err((StatusCode::CONFLICT, "device already exists".to_string()));
    }

    sqlx::query(
        r"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(prepared.device_id)
    .bind(prepared.account_id)
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
    .bind(prepared.account_id)
    .bind(prepared.device_id)
    .bind(serde_json::to_value(&prepared.delegation_envelope).map_err(internal_error)?)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(AddDeviceResponse {
        device_id: prepared.device_id,
        device_kid: prepared.device_kid,
    }))
}

async fn prepare_device_add(
    pool: &PgPool,
    payload: AddDeviceRequest,
) -> Result<PreparedDeviceAdd, (StatusCode, String)> {
    let account =
        sqlx::query_as::<_, AccountRow>("SELECT root_kid, root_pubkey FROM accounts WHERE id = $1")
            .bind(payload.account_id)
            .fetch_optional(pool)
            .await
            .map_err(internal_error)?;

    let Some(account) = account else {
        return Err((StatusCode::NOT_FOUND, "account not found".to_string()));
    };

    let root_pubkey_bytes = decode_key(&account.root_pubkey, "root_pubkey")?;
    let expected_root_kid = derive_kid(&root_pubkey_bytes);
    if expected_root_kid != account.root_kid {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored root_kid does not match pubkey".to_string(),
        ));
    }

    let device_pubkey_bytes = decode_key(&payload.device_pubkey, "device_pubkey")?;
    let device_kid = derive_kid(&device_pubkey_bytes);
    let device_id = extract_device_id(&payload.delegation_envelope)?;

    if let Some(envelope_account) = payload.delegation_envelope.signer.account_id {
        if envelope_account != payload.account_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "delegation account_id mismatch".to_string(),
            ));
        }
    }

    if payload.delegation_envelope.signer.kid != expected_root_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "delegation signer kid does not match current root".to_string(),
        ));
    }

    verify_envelope(&payload.delegation_envelope, &root_pubkey_bytes).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid delegation signature: {err}"),
        )
    })?;

    let latest_seq: Option<(i64,)> = sqlx::query_as(
        "SELECT seqno FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(payload.account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let next_seqno = match latest_seq {
        Some((seqno,)) => seqno + 1,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                "account has no existing sigchain entries".to_string(),
            ))
        }
    };

    let (device_name, device_type) = payload.device_metadata.map_or_else(
        || (None, "other".to_string()),
        |meta| {
            let name = meta.name;
            let device_type = meta.r#type.unwrap_or_else(|| "other".to_string());
            (name, device_type)
        },
    );

    Ok(PreparedDeviceAdd {
        account_id: payload.account_id,
        root_pubkey_bytes,
        device_pubkey_b64: payload.device_pubkey,
        device_kid,
        device_id,
        device_name,
        device_type,
        delegation_envelope: payload.delegation_envelope,
        next_seqno,
    })
}
