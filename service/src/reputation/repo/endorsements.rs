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
    #[error("endorsement already exists for this subject and topic")]
    Duplicate,
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
/// Returns `Duplicate` if the subject already has an endorsement for this topic.
/// Returns `Database` on connection or query failure.
pub async fn create_endorsement<'e, E>(
    executor: E,
    subject_id: Uuid,
    topic: &str,
    endorser_id: Option<Uuid>,
    evidence: Option<&serde_json::Value>,
) -> Result<CreatedEndorsement, EndorsementRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let id = Uuid::new_v4();

    let result = sqlx::query(
        r"
        INSERT INTO reputation__endorsements (id, subject_id, topic, endorser_id, evidence)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(id)
    .bind(subject_id)
    .bind(topic)
    .bind(endorser_id)
    .bind(evidence)
    .execute(executor)
    .await;

    match result {
        Ok(_) => Ok(CreatedEndorsement {
            id,
            subject_id,
            topic: topic.to_string(),
        }),
        Err(e) => {
            if let sqlx::Error::Database(ref db_err) = e {
                if let Some(constraint) = db_err.constraint() {
                    if constraint == "uq_endorsements_subject_topic_endorser"
                        || constraint == "uq_endorsements_genesis"
                    {
                        return Err(EndorsementRepoError::Duplicate);
                    }
                }
            }
            Err(EndorsementRepoError::Database(e))
        }
    }
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
