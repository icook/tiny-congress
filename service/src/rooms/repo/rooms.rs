//! Room persistence operations

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RoomRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub eligibility_topic: String,
    pub status: String,
    pub poll_duration_secs: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub constraint_type: String,
    pub constraint_config: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum RoomRepoError {
    #[error("room name already exists")]
    DuplicateName,
    #[error("room not found")]
    NotFound,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct RoomRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    eligibility_topic: String,
    status: String,
    poll_duration_secs: Option<i32>,
    created_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    constraint_type: String,
    constraint_config: serde_json::Value,
}

fn row_to_record(row: RoomRow) -> RoomRecord {
    RoomRecord {
        id: row.id,
        name: row.name,
        description: row.description,
        eligibility_topic: row.eligibility_topic,
        status: row.status,
        poll_duration_secs: row.poll_duration_secs,
        created_at: row.created_at,
        closed_at: row.closed_at,
        constraint_type: row.constraint_type,
        constraint_config: row.constraint_config,
    }
}

/// # Errors
///
/// Returns `DuplicateName` if a room with this name already exists.
pub async fn create_room<'e, E>(
    executor: E,
    name: &str,
    description: Option<&str>,
    eligibility_topic: &str,
    poll_duration_secs: Option<i32>,
) -> Result<RoomRecord, RoomRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = sqlx::query_as::<_, RoomRow>(
        r"
        INSERT INTO rooms__rooms
            (name, description, eligibility_topic, poll_duration_secs, constraint_type, constraint_config)
        VALUES ($1, $2, $3, $4, 'endorsed_by', jsonb_build_object('topic', $3))
        RETURNING id, name, description, eligibility_topic, status, poll_duration_secs, created_at, closed_at,
                  constraint_type, constraint_config
        ",
    )
    .bind(name)
    .bind(description)
    .bind(eligibility_topic)
    .bind(poll_duration_secs)
    .fetch_one(executor)
    .await;

    match result {
        Ok(row) => Ok(row_to_record(row)),
        Err(e) => {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint() == Some("uq_rooms_name") {
                    return Err(RoomRepoError::DuplicateName);
                }
            }
            Err(RoomRepoError::Database(e))
        }
    }
}

/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn list_rooms<'e, E>(
    executor: E,
    status_filter: Option<&str>,
) -> Result<Vec<RoomRecord>, RoomRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = if let Some(status) = status_filter {
        sqlx::query_as::<_, RoomRow>(
            r"
            SELECT id, name, description, eligibility_topic, status, poll_duration_secs,
                   created_at, closed_at, constraint_type, constraint_config
            FROM rooms__rooms WHERE status = $1 ORDER BY created_at DESC
            ",
        )
        .bind(status)
        .fetch_all(executor)
        .await?
    } else {
        sqlx::query_as::<_, RoomRow>(
            r"
            SELECT id, name, description, eligibility_topic, status, poll_duration_secs,
                   created_at, closed_at, constraint_type, constraint_config
            FROM rooms__rooms ORDER BY created_at DESC
            ",
        )
        .fetch_all(executor)
        .await?
    };

    Ok(rows.into_iter().map(row_to_record).collect())
}

/// # Errors
///
/// Returns `NotFound` if no room exists with this ID.
pub async fn get_room<'e, E>(executor: E, room_id: Uuid) -> Result<RoomRecord, RoomRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RoomRow>(
        r"
        SELECT id, name, description, eligibility_topic, status, poll_duration_secs,
               created_at, closed_at, constraint_type, constraint_config
        FROM rooms__rooms WHERE id = $1
        ",
    )
    .bind(room_id)
    .fetch_optional(executor)
    .await?
    .map_or_else(|| Err(RoomRepoError::NotFound), |r| Ok(row_to_record(r)))
}

/// # Errors
///
/// Returns `NotFound` if no room exists with this ID.
pub async fn update_room_status<'e, E>(
    executor: E,
    room_id: Uuid,
    status: &str,
) -> Result<(), RoomRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let closed_at = if status == "closed" || status == "archived" {
        Some(Utc::now())
    } else {
        None
    };

    let result = sqlx::query(
        r"UPDATE rooms__rooms SET status = $1, closed_at = COALESCE($2, closed_at) WHERE id = $3",
    )
    .bind(status)
    .bind(closed_at)
    .bind(room_id)
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(RoomRepoError::NotFound);
    }
    Ok(())
}

/// Find rooms that are open, have a cadence configured, and have no active or draft polls.
///
/// These rooms need new content to keep the lifecycle engine running.
///
/// # Errors
///
/// Returns `Database` on connection failure.
pub async fn rooms_needing_content<'e, E>(executor: E) -> Result<Vec<RoomRecord>, RoomRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query_as::<_, RoomRow>(
        r"
        SELECT r.id, r.name, r.description, r.eligibility_topic, r.status,
               r.poll_duration_secs, r.created_at, r.closed_at,
               r.constraint_type, r.constraint_config
        FROM rooms__rooms r
        WHERE r.status = 'open'
          AND r.poll_duration_secs IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM rooms__polls p
              WHERE p.room_id = r.id AND p.status IN ('active', 'draft')
          )
        ",
    )
    .fetch_all(executor)
    .await?;

    Ok(rows.into_iter().map(row_to_record).collect())
}
