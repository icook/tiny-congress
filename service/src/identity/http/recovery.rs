use axum::extract::{Extension, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

use crate::identity::crypto::{derive_kid, verify_envelope, SignedEnvelope};
use crate::identity::repo::event_store::{
    append_signed_event, append_signed_event_in_tx, AppendEventInput,
};

use super::accounts::{decode_key, internal_error};

#[derive(Debug, Deserialize)]
pub struct RecoveryPolicyRequest {
    pub account_id: Uuid,
    pub envelope: SignedEnvelope,
}

#[derive(Debug, Serialize)]
pub struct RecoveryPolicyResponse {
    pub policy_id: Uuid,
    pub threshold: i32,
    pub helpers: Vec<RecoveryHelper>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryHelper {
    pub helper_account_id: Uuid,
    pub helper_root_kid: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecoveryPolicyQuery {
    pub account_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct RecoveryPolicyView {
    pub policy_id: Uuid,
    pub threshold: i32,
    pub helpers: Vec<RecoveryHelper>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct RecoveryApprovalRequest {
    pub account_id: Uuid,
    pub helper_account_id: Uuid,
    pub helper_device_id: Uuid,
    pub policy_id: Uuid,
    pub envelope: SignedEnvelope,
}

#[derive(Debug, Serialize)]
pub struct RecoveryApprovalResponse {
    pub approval_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct RootRotationRequest {
    pub account_id: Uuid,
    pub envelope: SignedEnvelope,
}

#[derive(Debug, Serialize)]
pub struct RootRotationResponse {
    pub new_root_kid: String,
}

enum PolicyAction {
    Set(ParsedPolicy),
    Revoke,
}

#[derive(Debug)]
struct ParsedPolicy {
    threshold: i32,
    helpers: Vec<RecoveryHelper>,
}

#[derive(Debug)]
struct AccountRoot {
    root_kid: String,
    root_pubkey_bytes: Vec<u8>,
}

#[derive(Debug)]
struct ActivePolicy {
    id: Uuid,
    threshold: i32,
    helpers: Vec<RecoveryHelper>,
    created_at: chrono::DateTime<chrono::Utc>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug)]
struct DeviceContext {
    device_pubkey_bytes: Vec<u8>,
    device_kid: String,
}

#[derive(Debug)]
struct ParsedApprovalPayload {
    policy_id: Uuid,
    new_root_kid: String,
    new_root_pubkey_b64: String,
    new_root_pubkey_bytes: Vec<u8>,
}

#[derive(Debug)]
struct ParsedRotationPayload {
    policy_id: Uuid,
    new_root_kid: String,
    new_root_pubkey_b64: String,
    new_root_pubkey_bytes: Vec<u8>,
}

/// Create or revoke a recovery policy using a root-signed envelope.
///
/// # Errors
/// Returns 4xx on validation/signature errors, 404 when the account or policy is missing, and 500 on persistence failures.
#[allow(clippy::too_many_lines)]
pub async fn set_recovery_policy(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<RecoveryPolicyRequest>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    let account_root = load_account_root(&pool, payload.account_id).await?;
    verify_root_envelope(
        &payload.envelope,
        payload.account_id,
        &account_root.root_kid,
        &account_root.root_pubkey_bytes,
    )?;

    let action = parse_policy_action(&payload.envelope)?;
    let next_seqno = next_seqno(&pool, payload.account_id).await?;

    match action {
        PolicyAction::Set(parsed) => {
            let policy_id = Uuid::new_v4();
            append_signed_event(
                &pool,
                AppendEventInput {
                    account_id: payload.account_id,
                    seqno: next_seqno,
                    event_type: "RecoveryPolicySet".to_string(),
                    envelope: payload.envelope.clone(),
                    signer_pubkey: &account_root.root_pubkey_bytes,
                },
            )
            .await
            .map_err(|err| map_conflict(&err))?;

            let mut tx = pool
                .begin()
                .await
                .map_err(|err| internal_error(anyhow::anyhow!(err)))?;

            sqlx::query(
                "UPDATE recovery_policies SET revoked_at = NOW() WHERE account_id = $1 AND revoked_at IS NULL",
            )
            .bind(payload.account_id)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            sqlx::query(
                r"
                INSERT INTO recovery_policies (id, account_id, threshold, helpers, envelope)
                VALUES ($1, $2, $3, $4, $5)
                ",
            )
            .bind(policy_id)
            .bind(payload.account_id)
            .bind(parsed.threshold)
            .bind(serde_json::to_value(&parsed.helpers).map_err(internal_error)?)
            .bind(serde_json::to_value(&payload.envelope).map_err(internal_error)?)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            tx.commit().await.map_err(internal_error)?;

            Ok(Json(RecoveryPolicyResponse {
                policy_id,
                threshold: parsed.threshold,
                helpers: parsed.helpers,
            })
            .into_response())
        }
        PolicyAction::Revoke => {
            let active_policy: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM recovery_policies WHERE account_id = $1 AND revoked_at IS NULL",
            )
            .bind(payload.account_id)
            .fetch_optional(&pool)
            .await
            .map_err(internal_error)?;

            let Some((policy_id,)) = active_policy else {
                return Err((
                    StatusCode::NOT_FOUND,
                    "no active recovery policy".to_string(),
                ));
            };

            append_signed_event(
                &pool,
                AppendEventInput {
                    account_id: payload.account_id,
                    seqno: next_seqno,
                    event_type: "RecoveryPolicyRevocation".to_string(),
                    envelope: payload.envelope.clone(),
                    signer_pubkey: &account_root.root_pubkey_bytes,
                },
            )
            .await
            .map_err(|err| map_conflict(&err))?;

            let updated = sqlx::query(
                "UPDATE recovery_policies SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(policy_id)
            .execute(&pool)
            .await
            .map_err(internal_error)?;

            if updated.rows_affected() == 0 {
                return Err((
                    StatusCode::CONFLICT,
                    "recovery policy already revoked".to_string(),
                ));
            }

            Ok(StatusCode::NO_CONTENT.into_response())
        }
    }
}

/// Fetch the active recovery policy for an account.
///
/// # Errors
/// Returns 404 when not found, 400 on bad input, and 500 on DB failures.
pub async fn get_recovery_policy(
    Query(params): Query<RecoveryPolicyQuery>,
    Extension(pool): Extension<PgPool>,
) -> Result<Json<RecoveryPolicyView>, (StatusCode, String)> {
    let policy = load_active_policy(&pool, params.account_id).await?;

    Ok(Json(RecoveryPolicyView {
        policy_id: policy.id,
        threshold: policy.threshold,
        helpers: policy.helpers,
        created_at: policy.created_at,
        revoked_at: policy.revoked_at,
    }))
}

/// Record a helper's approval for rotating the root key.
///
/// # Errors
/// Returns 4xx for validation or policy mismatches and 500 on persistence failures.
pub async fn approve_recovery(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<RecoveryApprovalRequest>,
) -> Result<Json<RecoveryApprovalResponse>, (StatusCode, String)> {
    let policy = load_active_policy(&pool, payload.account_id).await?;
    if payload.policy_id != policy.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "policy_id does not match active policy".to_string(),
        ));
    }

    let helper_root = load_account_root(&pool, payload.helper_account_id).await?;
    let helper_entry = find_helper_entry(&policy.helpers, payload.helper_account_id)?;
    if let Some(expected) = &helper_entry.helper_root_kid {
        if expected != &helper_root.root_kid {
            return Err((
                StatusCode::FORBIDDEN,
                "helper root_kid does not match policy pin".to_string(),
            ));
        }
    }

    let helper_device = load_device_context(
        &pool,
        payload.helper_account_id,
        payload.helper_device_id,
        &helper_root.root_pubkey_bytes,
    )
    .await?;

    let approval_payload = parse_approval_payload(&payload.envelope)?;
    if approval_payload.policy_id != policy.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "approval policy_id does not match active policy".to_string(),
        ));
    }

    verify_helper_envelope(
        &payload.envelope,
        payload.helper_account_id,
        payload.helper_device_id,
        &helper_device.device_kid,
        &helper_device.device_pubkey_bytes,
    )?;

    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM recovery_approvals WHERE policy_id = $1 AND helper_account_id = $2",
    )
    .bind(policy.id)
    .bind(payload.helper_account_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?;

    if existing.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "helper already approved this policy".to_string(),
        ));
    }

    let next_seqno = next_seqno(&pool, payload.account_id).await?;
    let approval_id = Uuid::new_v4();

    let mut tx = pool
        .begin()
        .await
        .map_err(|err| internal_error(anyhow::anyhow!(err)))?;

    append_signed_event_in_tx(
        &mut tx,
        AppendEventInput {
            account_id: payload.account_id,
            seqno: next_seqno,
            event_type: "RecoveryApproval".to_string(),
            envelope: payload.envelope.clone(),
            signer_pubkey: &helper_device.device_pubkey_bytes,
        },
    )
    .await
    .map_err(|err| map_conflict(&err))?;

    sqlx::query(
        r"
        INSERT INTO recovery_approvals (id, account_id, policy_id, new_root_kid, new_root_pubkey, helper_account_id, helper_device_id, envelope)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ",
    )
    .bind(approval_id)
    .bind(payload.account_id)
    .bind(policy.id)
    .bind(&approval_payload.new_root_kid)
    .bind(&approval_payload.new_root_pubkey_b64)
    .bind(payload.helper_account_id)
    .bind(payload.helper_device_id)
    .bind(serde_json::to_value(&payload.envelope).map_err(internal_error)?)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(RecoveryApprovalResponse { approval_id }))
}

/// Rotate the root key after threshold helper approvals.
///
/// # Errors
/// Returns 4xx when approvals are missing/mismatched or signatures fail and 500 on persistence errors.
#[allow(clippy::too_many_lines)]
pub async fn rotate_root(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<RootRotationRequest>,
) -> Result<Json<RootRotationResponse>, (StatusCode, String)> {
    let policy = load_active_policy(&pool, payload.account_id).await?;
    let parsed = parse_rotation_payload(&payload.envelope)?;

    if parsed.policy_id != policy.id {
        return Err((
            StatusCode::BAD_REQUEST,
            "rotation policy_id does not match active policy".to_string(),
        ));
    }

    if let Some(envelope_account) = payload.envelope.signer.account_id {
        if envelope_account != payload.account_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "rotation envelope account_id mismatch".to_string(),
            ));
        }
    }

    let approvals = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT helper_account_id, new_root_kid, new_root_pubkey FROM recovery_approvals WHERE account_id = $1 AND policy_id = $2",
    )
    .bind(payload.account_id)
    .bind(policy.id)
    .fetch_all(&pool)
    .await
    .map_err(internal_error)?;

    if approvals.is_empty() {
        return Err((
            StatusCode::CONFLICT,
            "no approvals recorded for active policy".to_string(),
        ));
    }

    let mut helper_set = HashSet::new();
    let mut approved_kid: Option<String> = None;
    let mut approved_pubkey: Option<String> = None;
    for (helper_id, kid, pubkey) in approvals {
        helper_set.insert(helper_id);
        if let Some(existing_kid) = &approved_kid {
            if existing_kid != &kid || approved_pubkey.as_deref() != Some(&pubkey) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "approvals disagree on new root".to_string(),
                ));
            }
        } else {
            approved_kid = Some(kid.clone());
            approved_pubkey = Some(pubkey.clone());
        }
    }

    let threshold = usize::try_from(policy.threshold).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "invalid policy threshold".to_string(),
        )
    })?;

    if helper_set.len() < threshold {
        return Err((
            StatusCode::CONFLICT,
            "insufficient approvals for rotation threshold".to_string(),
        ));
    }

    if approved_kid.as_deref() != Some(parsed.new_root_kid.as_str())
        || approved_pubkey.as_deref() != Some(parsed.new_root_pubkey_b64.as_str())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "rotation envelope new_root does not match approvals".to_string(),
        ));
    }

    verify_envelope(&payload.envelope, &parsed.new_root_pubkey_bytes).map_err(|err| {
        (
            StatusCode::FORBIDDEN,
            format!("invalid rotation signature: {err}"),
        )
    })?;

    let next_seqno = next_seqno(&pool, payload.account_id).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|err| internal_error(anyhow::anyhow!(err)))?;

    append_signed_event_in_tx(
        &mut tx,
        AppendEventInput {
            account_id: payload.account_id,
            seqno: next_seqno,
            event_type: "RootRotation".to_string(),
            envelope: payload.envelope.clone(),
            signer_pubkey: &parsed.new_root_pubkey_bytes,
        },
    )
    .await
    .map_err(|err| map_conflict(&err))?;

    sqlx::query("UPDATE accounts SET root_kid = $1, root_pubkey = $2 WHERE id = $3")
        .bind(&parsed.new_root_kid)
        .bind(&parsed.new_root_pubkey_b64)
        .bind(payload.account_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;

    sqlx::query(
        "UPDATE device_delegations SET revoked_at = NOW() WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(payload.account_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(RootRotationResponse {
        new_root_kid: parsed.new_root_kid,
    }))
}

#[derive(sqlx::FromRow)]
struct RecoveryPolicyRow {
    id: Uuid,
    threshold: i32,
    helpers: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn load_active_policy(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<ActivePolicy, (StatusCode, String)> {
    let row = sqlx::query_as::<_, RecoveryPolicyRow>(
        "SELECT id, threshold, helpers, created_at, revoked_at FROM recovery_policies WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let Some(policy) = row else {
        return Err((
            StatusCode::NOT_FOUND,
            "recovery policy not found".to_string(),
        ));
    };

    let helpers: Vec<RecoveryHelper> = serde_json::from_value(policy.helpers)
        .map_err(|err| internal_error(anyhow::anyhow!(err)))?;

    Ok(ActivePolicy {
        id: policy.id,
        threshold: policy.threshold,
        helpers,
        created_at: policy.created_at,
        revoked_at: policy.revoked_at,
    })
}

fn verify_root_envelope(
    envelope: &SignedEnvelope,
    account_id: Uuid,
    expected_root_kid: &str,
    root_pubkey_bytes: &[u8],
) -> Result<(), (StatusCode, String)> {
    if let Some(envelope_account) = envelope.signer.account_id {
        if envelope_account != account_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "envelope account_id mismatch".to_string(),
            ));
        }
    }

    if envelope.signer.kid != expected_root_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "envelope signer kid does not match current root".to_string(),
        ));
    }

    verify_envelope(envelope, root_pubkey_bytes).map_err(|err| {
        (
            StatusCode::FORBIDDEN,
            format!("invalid envelope signature: {err}"),
        )
    })
}

fn parse_policy_action(envelope: &SignedEnvelope) -> Result<PolicyAction, (StatusCode, String)> {
    match envelope.payload_type.as_str() {
        "RecoveryPolicy" | "RecoveryPolicySet" => {
            let threshold = envelope
                .payload
                .get("threshold")
                .and_then(Value::as_i64)
                .ok_or_else(|| (StatusCode::BAD_REQUEST, "threshold is required".to_string()))?;

            if threshold < 1 {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "threshold must be at least 1".to_string(),
                ));
            }

            let helpers_value = envelope
                .payload
                .get("helpers")
                .ok_or_else(|| (StatusCode::BAD_REQUEST, "helpers is required".to_string()))?;
            let helpers_array = helpers_value.as_array().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "helpers must be an array".to_string(),
                )
            })?;

            let helpers = parse_helpers(helpers_array)?;

            let threshold_usize = usize::try_from(threshold).map_err(|_| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "threshold is too large".to_string(),
                )
            })?;

            if threshold_usize > helpers.len() {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "threshold cannot exceed helper count".to_string(),
                ));
            }

            let threshold_i32 = i32::try_from(threshold).map_err(|_| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "threshold is too large".to_string(),
                )
            })?;

            Ok(PolicyAction::Set(ParsedPolicy {
                threshold: threshold_i32,
                helpers,
            }))
        }
        "RecoveryPolicyRevocation" => Ok(PolicyAction::Revoke),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "unsupported payload_type for recovery policy".to_string(),
        )),
    }
}

fn find_helper_entry(
    helpers: &[RecoveryHelper],
    helper_account_id: Uuid,
) -> Result<&RecoveryHelper, (StatusCode, String)> {
    helpers
        .iter()
        .find(|h| h.helper_account_id == helper_account_id)
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                "helper is not authorized for this policy".to_string(),
            )
        })
}

fn parse_approval_payload(
    envelope: &SignedEnvelope,
) -> Result<ParsedApprovalPayload, (StatusCode, String)> {
    let policy_id = envelope
        .payload
        .get("policy_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "policy_id is required in approval payload".to_string(),
            )
        })
        .and_then(|raw| {
            Uuid::parse_str(raw).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "policy_id must be a UUID".to_string(),
                )
            })
        })?;

    let new_root_kid = envelope
        .payload
        .get("new_root_kid")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "new_root_kid is required".to_string(),
            )
        })?
        .to_string();

    let new_root_pubkey_b64 = envelope
        .payload
        .get("new_root_pubkey")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "new_root_pubkey is required".to_string(),
            )
        })?
        .to_string();

    let new_root_pubkey_bytes = decode_key(&new_root_pubkey_b64, "new_root_pubkey")?;
    let derived_kid = derive_kid(&new_root_pubkey_bytes);
    if derived_kid != new_root_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "new_root_kid does not match derived kid".to_string(),
        ));
    }

    Ok(ParsedApprovalPayload {
        policy_id,
        new_root_kid,
        new_root_pubkey_b64,
        new_root_pubkey_bytes,
    })
}

fn parse_rotation_payload(
    envelope: &SignedEnvelope,
) -> Result<ParsedRotationPayload, (StatusCode, String)> {
    let parsed = parse_approval_payload(envelope)?;
    Ok(ParsedRotationPayload {
        policy_id: parsed.policy_id,
        new_root_kid: parsed.new_root_kid,
        new_root_pubkey_b64: parsed.new_root_pubkey_b64,
        new_root_pubkey_bytes: parsed.new_root_pubkey_bytes,
    })
}

fn parse_helpers(array: &[Value]) -> Result<Vec<RecoveryHelper>, (StatusCode, String)> {
    let mut helpers = Vec::with_capacity(array.len());
    let mut seen_accounts = HashSet::new();

    for value in array {
        let helper_account_id = value
            .get("helper_account_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "helper_account_id is required".to_string(),
                )
            })
            .and_then(|raw| {
                Uuid::parse_str(raw).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "helper_account_id must be a UUID".to_string(),
                    )
                })
            })?;

        if !seen_accounts.insert(helper_account_id) {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "duplicate helper_account_id".to_string(),
            ));
        }

        let helper_root_kid = value
            .get("helper_root_kid")
            .and_then(Value::as_str)
            .map(str::to_string);

        helpers.push(RecoveryHelper {
            helper_account_id,
            helper_root_kid,
        });
    }

    Ok(helpers)
}

async fn load_account_root(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<AccountRoot, (StatusCode, String)> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT root_kid, root_pubkey FROM accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let Some((root_kid, root_pubkey_b64)) = row else {
        return Err((StatusCode::NOT_FOUND, "account not found".to_string()));
    };

    let root_pubkey_bytes = decode_key(&root_pubkey_b64, "root_pubkey")?;
    let derived = derive_kid(&root_pubkey_bytes);
    if derived != root_kid {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored root_kid does not match pubkey".to_string(),
        ));
    }

    Ok(AccountRoot {
        root_kid,
        root_pubkey_bytes,
    })
}

async fn load_device_context(
    pool: &PgPool,
    account_id: Uuid,
    device_id: Uuid,
    root_pubkey_bytes: &[u8],
) -> Result<DeviceContext, (StatusCode, String)> {
    let device = sqlx::query_as::<_, (String, String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT device_kid, device_pubkey, revoked_at FROM devices WHERE id = $1 AND account_id = $2",
    )
    .bind(device_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "device not found".to_string()))?;

    if device.2.is_some() {
        return Err((StatusCode::FORBIDDEN, "device revoked".to_string()));
    }

    let delegation = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT delegation_envelope FROM device_delegations WHERE account_id = $1 AND device_id = $2 AND revoked_at IS NULL",
    )
    .bind(account_id)
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| {
        (
            StatusCode::FORBIDDEN,
            "no active delegation for device".to_string(),
        )
    })?;

    let delegation_envelope: SignedEnvelope =
        serde_json::from_value(delegation.0).map_err(|err| internal_error(anyhow::anyhow!(err)))?;
    verify_envelope(&delegation_envelope, root_pubkey_bytes)
        .map_err(|err| (StatusCode::FORBIDDEN, format!("invalid delegation: {err}")))?;

    let device_pubkey_bytes = decode_key(&device.1, "device_pubkey")?;
    Ok(DeviceContext {
        device_pubkey_bytes,
        device_kid: device.0,
    })
}

async fn next_seqno(pool: &PgPool, account_id: Uuid) -> Result<i64, (StatusCode, String)> {
    let last = sqlx::query_as::<_, (i64,)>(
        "SELECT seqno FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    let Some((seqno,)) = last else {
        return Err((
            StatusCode::BAD_REQUEST,
            "account has no existing sigchain entries".to_string(),
        ));
    };

    Ok(seqno + 1)
}

fn verify_helper_envelope(
    envelope: &SignedEnvelope,
    helper_account_id: Uuid,
    helper_device_id: Uuid,
    expected_kid: &str,
    helper_pubkey: &[u8],
) -> Result<(), (StatusCode, String)> {
    if let Some(envelope_account) = envelope.signer.account_id {
        if envelope_account != helper_account_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "approval envelope account_id mismatch".to_string(),
            ));
        }
    }

    if let Some(envelope_device) = envelope.signer.device_id {
        if envelope_device != helper_device_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "approval envelope device_id mismatch".to_string(),
            ));
        }
    }

    if envelope.signer.kid != expected_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "approval signer kid does not match helper device".to_string(),
        ));
    }

    verify_envelope(envelope, helper_pubkey).map_err(|err| {
        (
            StatusCode::FORBIDDEN,
            format!("invalid approval signature: {err}"),
        )
    })
}

fn map_conflict(err: &anyhow::Error) -> (StatusCode, String) {
    (
        StatusCode::CONFLICT,
        format!("sigchain append failed: {err}"),
    )
}
