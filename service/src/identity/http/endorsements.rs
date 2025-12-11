use anyhow::anyhow;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::identity::crypto::{derive_kid, encode_base64url, verify_envelope, SignedEnvelope};
use crate::identity::repo::event_store::{append_signed_event, AppendEventInput};

use super::accounts::{decode_key, internal_error};

#[derive(Debug, Deserialize)]
pub struct EndorsementCreateRequest {
    pub account_id: Uuid,
    pub device_id: Uuid,
    pub envelope: SignedEnvelope,
}

#[derive(Debug, Serialize)]
pub struct EndorsementCreateResponse {
    pub endorsement_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct EndorsementRevokeRequest {
    pub account_id: Uuid,
    pub device_id: Uuid,
    pub envelope: SignedEnvelope,
}

#[derive(Debug, Deserialize)]
pub struct ListEndorsementsParams {
    pub topic: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EndorsementRecord {
    pub id: Uuid,
    pub author_account_id: Uuid,
    pub author_device_id: Uuid,
    pub subject_type: String,
    pub subject_id: String,
    pub topic: String,
    pub magnitude: f64,
    pub confidence: f64,
    pub context: Option<String>,
    pub tags: Option<Vec<String>>,
    pub evidence_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EndorsementAggregate {
    pub subject_type: String,
    pub subject_id: String,
    pub topic: String,
    pub n_total: i32,
    pub n_pos: i32,
    pub n_neg: i32,
    pub sum_weight: f64,
    pub weighted_mean: Option<f64>,
}

#[derive(Debug)]
struct ParsedEndorsementPayload {
    subject_type: String,
    subject_id: String,
    topic: String,
    magnitude: f64,
    confidence: f64,
    context: Option<String>,
    tags: Option<Vec<String>>,
    evidence_url: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SignedEventRow {
    canonical_bytes_hash: Vec<u8>,
}

/// Create an endorsement from a device-signed envelope.
///
/// # Errors
/// Returns 4xx when validation, delegation, or signature checks fail; 500 on persistence errors.
pub async fn create_endorsement(
    Extension(pool): Extension<PgPool>,
    Json(payload): Json<EndorsementCreateRequest>,
) -> Result<Json<EndorsementCreateResponse>, (StatusCode, String)> {
    let context = load_account_device_context(&pool, payload.account_id, payload.device_id).await?;
    verify_device_envelope(
        &payload.envelope,
        &context.device_pubkey_bytes,
        &context.device_kid,
        payload.account_id,
        payload.device_id,
    )?;
    ensure_prev_hash_matches(&pool, payload.account_id, &payload.envelope).await?;

    let parsed = parse_endorsement_payload(&payload.envelope.payload)?;
    let endorsement_id = Uuid::new_v4();
    let next_seqno = next_seqno(&pool, payload.account_id).await?;

    append_signed_event(
        &pool,
        AppendEventInput {
            account_id: payload.account_id,
            seqno: next_seqno,
            event_type: "EndorsementCreated".to_string(),
            envelope: payload.envelope.clone(),
            signer_pubkey: &context.device_pubkey_bytes,
        },
    )
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r"
        INSERT INTO endorsements (
            id, author_account_id, author_device_id, subject_type, subject_id, topic, magnitude, confidence, context, tags, evidence_url, envelope
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ",
    )
    .bind(endorsement_id)
    .bind(payload.account_id)
    .bind(payload.device_id)
    .bind(&parsed.subject_type)
    .bind(&parsed.subject_id)
    .bind(&parsed.topic)
    .bind(parsed.magnitude)
    .bind(parsed.confidence)
    .bind(parsed.context)
    .bind(parsed.tags)
    .bind(parsed.evidence_url)
    .bind(serde_json::to_value(&payload.envelope).map_err(internal_error)?)
    .execute(&pool)
    .await
    .map_err(internal_error)?;

    recompute_aggregate_and_reputation(
        &pool,
        &parsed.subject_type,
        &parsed.subject_id,
        &parsed.topic,
    )
    .await?;

    Ok(Json(EndorsementCreateResponse { endorsement_id }))
}

/// Revoke an endorsement with a device-signed revocation envelope.
///
/// # Errors
/// Returns 4xx when validation fails or endorsement not found; 500 on persistence errors.
pub async fn revoke_endorsement(
    Extension(pool): Extension<PgPool>,
    Path(endorsement_id): Path<Uuid>,
    Json(payload): Json<EndorsementRevokeRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let context = load_account_device_context(&pool, payload.account_id, payload.device_id).await?;
    verify_device_envelope(
        &payload.envelope,
        &context.device_pubkey_bytes,
        &context.device_kid,
        payload.account_id,
        payload.device_id,
    )?;
    ensure_prev_hash_matches(&pool, payload.account_id, &payload.envelope).await?;

    let endorsement = sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT author_device_id, revoked_at FROM endorsements WHERE id = $1 AND author_account_id = $2",
    )
    .bind(endorsement_id)
    .bind(payload.account_id)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "endorsement not found".to_string()))?;

    if endorsement.1.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "endorsement already revoked".to_string(),
        ));
    }

    if endorsement.0 != payload.device_id {
        return Err((
            StatusCode::FORBIDDEN,
            "revocation must be signed by authoring device".to_string(),
        ));
    }

    let next_seqno = next_seqno(&pool, payload.account_id).await?;
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id: payload.account_id,
            seqno: next_seqno,
            event_type: "EndorsementRevocation".to_string(),
            envelope: payload.envelope.clone(),
            signer_pubkey: &context.device_pubkey_bytes,
        },
    )
    .await
    .map_err(internal_error)?;

    let updated = sqlx::query(
        "UPDATE endorsements SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(endorsement_id)
    .execute(&pool)
    .await
    .map_err(internal_error)?;

    if updated.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            "endorsement already revoked".to_string(),
        ));
    }

    let (subject_type, subject_id, topic) = sqlx::query_as::<_, (String, String, String)>(
        "SELECT subject_type, subject_id, topic FROM endorsements WHERE id = $1",
    )
    .bind(endorsement_id)
    .fetch_one(&pool)
    .await
    .map_err(internal_error)?;

    recompute_aggregate_and_reputation(&pool, &subject_type, &subject_id, &topic).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// List active endorsements for a subject, optionally filtered by topic, with aggregate.
///
/// # Errors
/// Returns a 4xx when the query parameters are invalid; 500 on DB errors.
pub async fn list_endorsements(
    Path(subject_id): Path<Uuid>,
    Query(params): Query<ListEndorsementsParams>,
    Extension(pool): Extension<PgPool>,
) -> Result<Json<(Vec<EndorsementRecord>, Option<EndorsementAggregate>)>, (StatusCode, String)> {
    let topic_filter = params.topic.clone();
    let rows = sqlx::query_as::<_, EndorsementRecord>(
        r"
        SELECT id, author_account_id, author_device_id, subject_type, subject_id, topic, magnitude, confidence, context, tags, evidence_url, created_at
        FROM endorsements
        WHERE subject_type = 'account' AND subject_id = $1 AND revoked_at IS NULL
        AND ($2::text IS NULL OR topic = $2)
        ORDER BY created_at DESC
        ",
    )
    .bind(subject_id.to_string())
    .bind(topic_filter.clone())
    .fetch_all(&pool)
    .await
    .map_err(internal_error)?;

    let aggregate = sqlx::query_as::<_, EndorsementAggregate>(
        r"
        SELECT subject_type, subject_id, topic, n_total, n_pos, n_neg, sum_weight, weighted_mean
        FROM endorsement_aggregates
        WHERE subject_type = 'account' AND subject_id = $1 AND ($2::text IS NULL OR topic = $2)
        ",
    )
    .bind(subject_id.to_string())
    .bind(topic_filter)
    .fetch_optional(&pool)
    .await
    .map_err(internal_error)?;

    Ok(Json((rows, aggregate)))
}

#[derive(Debug)]
struct AccountDeviceContext {
    device_pubkey_bytes: Vec<u8>,
    device_kid: String,
}

async fn load_account_device_context(
    pool: &PgPool,
    account_id: Uuid,
    device_id: Uuid,
) -> Result<AccountDeviceContext, (StatusCode, String)> {
    let account = sqlx::query_as::<_, (String, String)>(
        "SELECT root_kid, root_pubkey FROM accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "account not found".to_string()))?;

    let root_pubkey_bytes = decode_key(&account.1, "root_pubkey")?;
    if derive_kid(&root_pubkey_bytes) != account.0 {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored root_kid does not match pubkey".to_string(),
        ));
    }

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
        serde_json::from_value(delegation.0).map_err(|e| internal_error(anyhow!(e)))?;
    verify_envelope(&delegation_envelope, &root_pubkey_bytes)
        .map_err(|err| (StatusCode::FORBIDDEN, format!("invalid delegation: {err}")))?;

    let device_pubkey_bytes = decode_key(&device.1, "device_pubkey")?;
    Ok(AccountDeviceContext {
        device_pubkey_bytes,
        device_kid: device.0,
    })
}

fn verify_device_envelope(
    envelope: &SignedEnvelope,
    device_pubkey: &[u8],
    device_kid: &str,
    account_id: Uuid,
    device_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    if let Some(envelope_account) = envelope.signer.account_id {
        if envelope_account != account_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "envelope account_id mismatch".to_string(),
            ));
        }
    }
    if let Some(envelope_device) = envelope.signer.device_id {
        if envelope_device != device_id {
            return Err((
                StatusCode::BAD_REQUEST,
                "envelope device_id mismatch".to_string(),
            ));
        }
    }
    if envelope.signer.kid != device_kid {
        return Err((
            StatusCode::BAD_REQUEST,
            "envelope signer kid does not match device".to_string(),
        ));
    }

    verify_envelope(envelope, device_pubkey).map_err(|err| {
        (
            StatusCode::FORBIDDEN,
            format!("invalid envelope signature: {err}"),
        )
    })?;
    Ok(())
}

fn parse_endorsement_payload(
    payload: &Value,
) -> Result<ParsedEndorsementPayload, (StatusCode, String)> {
    let subject_type = as_string(payload, "subject_type")?;
    let subject_id = as_string(payload, "subject_id")?;
    let topic = as_string(payload, "topic")?;
    let magnitude = as_f64(payload, "magnitude")?;
    let confidence = as_f64(payload, "confidence")?;

    if !(0.0..=1.0).contains(&confidence) {
        return Err((
            StatusCode::BAD_REQUEST,
            "confidence must be between 0 and 1".to_string(),
        ));
    }
    if !(-1.0..=1.0).contains(&magnitude) {
        return Err((
            StatusCode::BAD_REQUEST,
            "magnitude must be between -1.0 and 1.0".to_string(),
        ));
    }

    let context = payload
        .get("context")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let tags = payload.get("tags").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    });
    let evidence_url = payload
        .get("evidence_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    Ok(ParsedEndorsementPayload {
        subject_type,
        subject_id,
        topic,
        magnitude,
        confidence,
        context,
        tags,
        evidence_url,
    })
}

fn as_string(payload: &Value, key: &str) -> Result<String, (StatusCode, String)> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("{key} is required and must be a string"),
            )
        })
}

fn as_f64(payload: &Value, key: &str) -> Result<f64, (StatusCode, String)> {
    payload.get(key).and_then(Value::as_f64).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            format!("{key} is required and must be a number"),
        )
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

    Ok(last.map_or(1, |row| row.0 + 1))
}

async fn ensure_prev_hash_matches(
    pool: &PgPool,
    account_id: Uuid,
    envelope: &SignedEnvelope,
) -> Result<(), (StatusCode, String)> {
    let last = sqlx::query_as::<_, SignedEventRow>(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?;

    if let Some(prev) = last {
        let expected_prev_hash = encode_base64url(&prev.canonical_bytes_hash);
        let provided = envelope
            .payload
            .get("prev_hash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if expected_prev_hash != provided {
            return Err((
                StatusCode::BAD_REQUEST,
                "prev_hash does not match last sigchain link".to_string(),
            ));
        }
    } else if envelope.payload.get("prev_hash").is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "first sigchain link must omit prev_hash".to_string(),
        ));
    }

    Ok(())
}

async fn recompute_aggregate_and_reputation(
    pool: &PgPool,
    subject_type: &str,
    subject_id: &str,
    topic: &str,
) -> Result<(), (StatusCode, String)> {
    let stats = sqlx::query_as::<_, (i64, i64, i64, f64, f64)>(
        r"
        SELECT
            COUNT(*) AS n_total,
            COUNT(*) FILTER (WHERE magnitude > 0) AS n_pos,
            COUNT(*) FILTER (WHERE magnitude < 0) AS n_neg,
            COALESCE(SUM(confidence), 0) AS sum_weight,
            COALESCE(SUM(magnitude * confidence), 0) AS weighted_sum
        FROM endorsements
        WHERE subject_type = $1 AND subject_id = $2 AND topic = $3 AND revoked_at IS NULL
        ",
    )
    .bind(subject_type)
    .bind(subject_id)
    .bind(topic)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;

    let weighted_mean = if stats.3 > 0.0 {
        Some(stats.4 / stats.3)
    } else {
        None
    };

    sqlx::query(
        r"
        INSERT INTO endorsement_aggregates (subject_type, subject_id, topic, n_total, n_pos, n_neg, sum_weight, weighted_mean)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (subject_type, subject_id, topic)
        DO UPDATE SET n_total = EXCLUDED.n_total,
                      n_pos = EXCLUDED.n_pos,
                      n_neg = EXCLUDED.n_neg,
                      sum_weight = EXCLUDED.sum_weight,
                      weighted_mean = EXCLUDED.weighted_mean,
                      updated_at = NOW()
        ",
    )
    .bind(subject_type)
    .bind(subject_id)
    .bind(topic)
    .bind(stats.0)
    .bind(stats.1)
    .bind(stats.2)
    .bind(stats.3)
    .bind(weighted_mean)
    .execute(pool)
    .await
    .map_err(internal_error)?;

    if subject_type == "account" {
        update_reputation(pool, subject_id).await?;
    }

    Ok(())
}

async fn update_reputation(pool: &PgPool, subject_id: &str) -> Result<(), (StatusCode, String)> {
    // Simple heuristic: base 0.5 plus average of selected topics weighted_mean * 0.25 each.
    let topics = vec!["trustworthy", "is_real_person"];
    let rows = sqlx::query_as::<_, (String, Option<f64>)>(
        "SELECT topic, weighted_mean FROM endorsement_aggregates WHERE subject_type = 'account' AND subject_id = $1 AND topic = ANY($2)",
    )
    .bind(subject_id)
    .bind(&topics)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    let mut sum = 0.5f64;
    for (_, mean) in rows {
        if let Some(val) = mean {
            sum += 0.25 * val.clamp(-1.0, 1.0);
        }
    }
    let score = sum.clamp(0.0, 1.0);

    sqlx::query(
        r"
        INSERT INTO reputation_scores (account_id, score, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (account_id)
        DO UPDATE SET score = EXCLUDED.score, updated_at = NOW()
        ",
    )
    .bind(subject_id)
    .bind(score)
    .execute(pool)
    .await
    .map_err(internal_error)?;

    Ok(())
}
