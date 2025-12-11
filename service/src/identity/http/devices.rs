use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::Json;
use base64::Engine;
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

#[derive(Debug)]
struct PreparedDeviceRevoke {
    account_id: Uuid,
    delegation_envelope: SignedEnvelope,
    root_pubkey_bytes: Vec<u8>,
    next_seqno: i64,
    reason: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct RevokeDeviceRequest {
    pub account_id: Uuid,
    pub delegation_envelope: SignedEnvelope,
    pub reason: Option<String>,
}

/// Revoke a device with a root-signed revocation link and update sigchain/read models.
///
/// # Errors
/// Returns 400 for validation/signature issues, 404 when the device or account is missing, 409 when already revoked, and 500 on persistence failures.
pub async fn revoke_device(
    Extension(pool): Extension<PgPool>,
    Path(device_id): Path<Uuid>,
    Json(payload): Json<RevokeDeviceRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let prepared = prepare_device_revoke(&pool, device_id, payload).await?;

    append_signed_event(
        &pool,
        AppendEventInput {
            account_id: prepared.account_id,
            seqno: prepared.next_seqno,
            event_type: "DeviceRevocation".to_string(),
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

    let updated = sqlx::query(
        r"
        UPDATE devices
        SET revoked_at = NOW(), revocation_reason = COALESCE($3, revocation_reason)
        WHERE account_id = $1 AND id = $2 AND revoked_at IS NULL
        ",
    )
    .bind(prepared.account_id)
    .bind(device_id)
    .bind(prepared.reason.clone())
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    if updated.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            "device already revoked or missing".to_string(),
        ));
    }

    sqlx::query(
        r"
        UPDATE device_delegations
        SET revoked_at = NOW()
        WHERE account_id = $1 AND device_id = $2 AND revoked_at IS NULL
        ",
    )
    .bind(prepared.account_id)
    .bind(device_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
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

async fn prepare_device_revoke(
    pool: &PgPool,
    device_id: Uuid,
    payload: RevokeDeviceRequest,
) -> Result<PreparedDeviceRevoke, (StatusCode, String)> {
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

    // Ensure the device exists and belongs to account
    let device_exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM devices WHERE account_id = $1 AND id = $2")
            .bind(payload.account_id)
            .bind(device_id)
            .fetch_optional(pool)
            .await
            .map_err(internal_error)?;

    if device_exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "device not found".to_string()));
    }

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
            format!("invalid revocation signature: {err}"),
        )
    })?;

    let latest_seq: Option<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT seqno, canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(payload.account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let (next_seqno, prev_hash) = match latest_seq {
        Some((seqno, prev_hash_bytes)) => (seqno + 1, prev_hash_bytes),
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                "account has no existing sigchain entries".to_string(),
            ))
        }
    };

    // Ensure prev_hash in envelope matches last link
    let expected_prev_hash = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(prev_hash);
    let envelope_prev_hash = payload
        .delegation_envelope
        .payload
        .get("prev_hash")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if envelope_prev_hash != expected_prev_hash {
        return Err((
            StatusCode::BAD_REQUEST,
            "prev_hash does not match latest sigchain entry".to_string(),
        ));
    }

    Ok(PreparedDeviceRevoke {
        account_id: payload.account_id,
        delegation_envelope: payload.delegation_envelope,
        root_pubkey_bytes,
        next_seqno,
        reason: payload.reason,
    })
}
