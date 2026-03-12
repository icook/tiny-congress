//! Endorsement persistence operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EndorsementRecord {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub topic: String,
    pub endorser_id: Option<Uuid>,
    pub evidence: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct CreatedEndorsement {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub topic: String,
}

// ─── Error type ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum EndorsementRepoError {
    #[error("endorsement not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

// ─── SQL row types ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct EndorsementRow {
    id: Uuid,
    subject_id: Uuid,
    topic: String,
    endorser_id: Option<Uuid>,
    evidence: Option<serde_json::Value>,
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

fn row_to_record(row: EndorsementRow) -> EndorsementRecord {
    EndorsementRecord {
        id: row.id,
        subject_id: row.subject_id,
        topic: row.topic,
        endorser_id: row.endorser_id,
        evidence: row.evidence,
        created_at: row.created_at,
        revoked_at: row.revoked_at,
    }
}

// ─── SQL operations ────────────────────────────────────────────────────────

/// # Errors
///
/// Returns `Database` on connection or query failure.
///
/// # Idempotency
///
/// Uses `ON CONFLICT DO UPDATE` on `(endorser_id, subject_id, topic)`, so re-running
/// with the same arguments updates the weight and attestation atomically instead of
/// returning an error. This idempotency applies only to non-genesis endorsements
/// (`endorser_id IS NOT NULL`). Genesis endorsements (`endorser_id = None`) use a
/// separate partial index and do not participate in this upsert path.
pub async fn create_endorsement<'e, E>(
    executor: E,
    subject_id: Uuid,
    topic: &str,
    endorser_id: Option<Uuid>,
    evidence: Option<&serde_json::Value>,
    weight: f32,
    attestation: Option<&serde_json::Value>,
) -> Result<CreatedEndorsement, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();

    let row: (Uuid,) = sqlx::query_as(
        r"
        INSERT INTO reputation__endorsements
            (id, subject_id, topic, endorser_id, evidence, weight, attestation)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (subject_id, topic, endorser_id)
            DO UPDATE SET weight = EXCLUDED.weight, attestation = EXCLUDED.attestation
        RETURNING id
        ",
    )
    .bind(id)
    .bind(subject_id)
    .bind(topic)
    .bind(endorser_id)
    .bind(evidence)
    .bind(weight)
    .bind(attestation)
    .fetch_one(executor)
    .await
    .map_err(EndorsementRepoError::Database)?;

    Ok(CreatedEndorsement {
        id: row.0,
        subject_id,
        topic: topic.to_string(),
    })
}

/// # Errors
///
/// Returns `Database` on connection or query failure.
pub async fn has_endorsement<'e, E>(
    executor: E,
    subject_id: Uuid,
    topic: &str,
) -> Result<bool, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let exists: bool = sqlx::query_scalar(
        r"
        SELECT EXISTS(
            SELECT 1 FROM reputation__endorsements
            WHERE subject_id = $1 AND topic = $2 AND revoked_at IS NULL
        )
        ",
    )
    .bind(subject_id)
    .bind(topic)
    .fetch_one(executor)
    .await?;

    Ok(exists)
}

/// # Errors
///
/// Returns `Database` on connection or query failure.
pub async fn list_endorsements_by_subject<'e, E>(
    executor: E,
    subject_id: Uuid,
) -> Result<Vec<EndorsementRecord>, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, EndorsementRow>(
        r"
        SELECT id, subject_id, topic, endorser_id, evidence, created_at, revoked_at
        FROM reputation__endorsements
        WHERE subject_id = $1
        ORDER BY created_at DESC
        ",
    )
    .bind(subject_id)
    .fetch_all(executor)
    .await?;

    Ok(rows.into_iter().map(row_to_record).collect())
}

/// Revoke the active endorsement from `endorser_id` to `subject_id` on `topic`.
///
/// A no-op if no active endorsement exists.
///
/// # Errors
///
/// Returns `Database` on connection or query failure.
pub async fn revoke_endorsement<'e, E>(
    executor: E,
    endorser_id: Uuid,
    subject_id: Uuid,
    topic: &str,
) -> Result<(), EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r"
        UPDATE reputation__endorsements
        SET revoked_at = NOW()
        WHERE endorser_id = $1 AND subject_id = $2 AND topic = $3 AND revoked_at IS NULL
        ",
    )
    .bind(endorser_id)
    .bind(subject_id)
    .bind(topic)
    .execute(executor)
    .await?;
    Ok(())
}

/// Count active (non-revoked) trust endorsements made by a given endorser.
///
/// # Errors
///
/// Returns `Database` on connection or query failure.
pub async fn count_active_trust_endorsements_by<'e, E>(
    executor: E,
    endorser_id: Uuid,
) -> Result<i64, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let count: i64 = sqlx::query_scalar(
        r"
        SELECT COUNT(*) FROM reputation__endorsements
        WHERE endorser_id = $1 AND topic = 'trust' AND revoked_at IS NULL
        ",
    )
    .bind(endorser_id)
    .fetch_one(executor)
    .await?;

    Ok(count)
}

/// # Errors
///
/// Returns `NotFound` if no endorsement exists for this subject and topic.
/// Returns `Database` on connection or query failure.
pub async fn get_endorsement_by_subject_and_topic<'e, E>(
    executor: E,
    subject_id: Uuid,
    topic: &str,
) -> Result<EndorsementRecord, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, EndorsementRow>(
        r"
        SELECT id, subject_id, topic, endorser_id, evidence, created_at, revoked_at
        FROM reputation__endorsements
        WHERE subject_id = $1 AND topic = $2
        ",
    )
    .bind(subject_id)
    .bind(topic)
    .fetch_optional(executor)
    .await?;

    row.map_or_else(
        || Err(EndorsementRepoError::NotFound),
        |r| Ok(row_to_record(r)),
    )
}
