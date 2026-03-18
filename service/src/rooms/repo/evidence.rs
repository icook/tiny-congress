//! Evidence persistence operations for poll dimensions

use uuid::Uuid;

// ─── Record types ──────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct EvidenceRecord {
    pub id: Uuid,
    pub dimension_id: Uuid,
    pub stance: String,
    pub claim: String,
    pub source: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct NewEvidence<'a> {
    pub stance: &'a str,
    pub claim: &'a str,
    pub source: Option<&'a str>,
}

// ─── Evidence operations ──────────────────────────────────────────────────

/// Insert multiple evidence rows for a single dimension using unnest arrays.
/// Returns the number of rows inserted.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure or constraint violation.
pub async fn insert_evidence<'e, E>(
    executor: E,
    dimension_id: Uuid,
    evidence: &[NewEvidence<'_>],
) -> Result<u64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    if evidence.is_empty() {
        return Ok(0);
    }

    let stances: Vec<&str> = evidence.iter().map(|e| e.stance).collect();
    let claims: Vec<&str> = evidence.iter().map(|e| e.claim).collect();
    let sources: Vec<Option<&str>> = evidence.iter().map(|e| e.source).collect();
    let count = evidence.len();
    let dim_ids: Vec<Uuid> = vec![dimension_id; count];

    let result = sqlx::query(
        r"
        INSERT INTO rooms__poll_evidence (dimension_id, stance, claim, source)
        SELECT * FROM UNNEST($1::uuid[], $2::text[], $3::text[], $4::text[])
        ",
    )
    .bind(&dim_ids)
    .bind(&stances)
    .bind(&claims)
    .bind(&sources)
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}

/// Get all evidence records for a set of dimension IDs.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn get_evidence_for_dimensions<'e, E>(
    executor: E,
    dimension_ids: &[Uuid],
) -> Result<Vec<EvidenceRecord>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    if dimension_ids.is_empty() {
        return Ok(vec![]);
    }

    sqlx::query_as::<_, EvidenceRecord>(
        r"
        SELECT id, dimension_id, stance, claim, source, created_at
        FROM rooms__poll_evidence
        WHERE dimension_id = ANY($1)
        ORDER BY dimension_id, stance DESC, created_at
        ",
    )
    .bind(dimension_ids)
    .fetch_all(executor)
    .await
}

/// Delete all evidence for dimensions belonging to a specific poll.
/// Used by ring buffer reset. Returns rows deleted.
///
/// # Errors
///
/// Returns `sqlx::Error` on connection failure.
pub async fn delete_evidence_for_poll<'e, E>(executor: E, poll_id: Uuid) -> Result<u64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query(
        r"
        DELETE FROM rooms__poll_evidence
        WHERE dimension_id IN (
            SELECT id FROM rooms__poll_dimensions WHERE poll_id = $1
        )
        ",
    )
    .bind(poll_id)
    .execute(executor)
    .await?;

    Ok(result.rows_affected())
}
