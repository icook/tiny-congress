use anyhow::{anyhow, Context};
use sha2::{Digest, Sha256};
use sqlx::{query, query_as, PgPool};
use uuid::Uuid;

use crate::identity::crypto::{verify_envelope, CryptoError, SignedEnvelope};

/// Input for appending a signed event to the per-account sigchain.
pub struct AppendEventInput<'a> {
    pub account_id: Uuid,
    pub seqno: i64,
    pub event_type: String,
    pub envelope: SignedEnvelope,
    pub signer_pubkey: &'a [u8],
}

/// Append a signed event while enforcing seqno, `prev_hash`, and signature validity.
///
/// # Errors
/// Returns an error when seqnos are out of order, `prev_hash` mismatches, canonicalization fails, or the insert fails.
pub async fn append_signed_event(
    pool: &PgPool,
    input: AppendEventInput<'_>,
) -> Result<(), anyhow::Error> {
    let mut tx = pool.begin().await?;

    let previous = query_as::<_, SignedEventRow>(
        r"
        SELECT seqno, event_type, canonical_bytes_hash, envelope_json
        FROM signed_events
        WHERE account_id = $1
        ORDER BY seqno DESC
        LIMIT 1
        ",
    )
    .bind(input.account_id)
    .fetch_optional(&mut *tx)
    .await?;

    match previous {
        Some(prev) => {
            if input.seqno != prev.seqno + 1 {
                return Err(anyhow!(
                    "seqno out of order: expected {}, got {}",
                    prev.seqno + 1,
                    input.seqno
                ));
            }

            let expected_prev_hash = input
                .envelope
                .prev_hash_bytes()
                .context("prev_hash missing for chained event")?;

            match expected_prev_hash {
                Some(value) if value == prev.canonical_bytes_hash => {}
                Some(_) => {
                    return Err(anyhow!("prev_hash does not match previous link"));
                }
                None => {
                    return Err(anyhow!("prev_hash missing for seqno > 1"));
                }
            }
        }
        None => {
            if input.seqno != 1 {
                return Err(anyhow!("first event must use seqno 1"));
            }
        }
    }

    verify_envelope(&input.envelope, input.signer_pubkey).map_err(|err| match err {
        CryptoError::InvalidKey(msg) | CryptoError::InvalidFormat(msg) => anyhow!(msg),
        other => anyhow!(other),
    })?;

    let canonical_bytes = input
        .envelope
        .canonical_signing_bytes()
        .context("failed to canonicalize envelope")?;
    let canonical_hash = Sha256::digest(&canonical_bytes).to_vec();

    query(
        r"
        INSERT INTO signed_events (account_id, seqno, event_type, canonical_bytes_hash, envelope_json)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(input.account_id)
    .bind(input.seqno)
    .bind(&input.event_type)
    .bind(&canonical_hash)
    .bind(serde_json::to_value(&input.envelope)?)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SignedEventRecord {
    pub seqno: i64,
    pub event_type: String,
    pub canonical_bytes_hash: Vec<u8>,
    pub envelope_json: serde_json::Value,
}

/// Fetch all signed events for an account ordered by seqno.
///
/// # Errors
/// Returns an error when the query fails.
pub async fn fetch_events(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<Vec<SignedEventRecord>, anyhow::Error> {
    let rows = query_as::<_, SignedEventRow>(
        r"
        SELECT seqno, event_type, canonical_bytes_hash, envelope_json
        FROM signed_events
        WHERE account_id = $1
        ORDER BY seqno ASC
        ",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| SignedEventRecord {
            seqno: row.seqno,
            event_type: row.event_type,
            canonical_bytes_hash: row.canonical_bytes_hash,
            envelope_json: row.envelope_json,
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct SignedEventRow {
    seqno: i64,
    event_type: String,
    canonical_bytes_hash: Vec<u8>,
    envelope_json: serde_json::Value,
}
