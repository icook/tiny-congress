//! Content insertion and querying for demo seed data.

use sqlx::PgPool;
use uuid::Uuid;

use crate::rooms::repo::{
    polls::{create_dimension, create_poll, update_poll_status},
    rooms::{create_room, RoomRepoError},
};

use super::llm::SeedContent;

/// A poll and its dimensions: `(poll_id, Vec<(dimension_id, min_value, max_value)>)`.
type PollWithDimensions = (Uuid, Vec<(Uuid, f32, f32)>);

/// Result of inserting seed content.
#[derive(Debug)]
pub struct InsertResult {
    pub rooms_created: usize,
    pub rooms_skipped: usize,
    pub polls_created: usize,
}

/// Insert LLM-generated rooms, polls, and dimensions into the database.
///
/// For each room in the seed content, a room is created with the
/// `"identity_verified"` eligibility topic. Duplicate room names are
/// silently skipped. Each room's polls and their dimensions are inserted,
/// and polls are activated once all dimensions are in place.
///
/// # Errors
///
/// Returns an error if a database operation fails for a reason other than
/// a duplicate room name.
pub async fn insert_seed_content(
    pool: &PgPool,
    content: &SeedContent,
) -> Result<InsertResult, anyhow::Error> {
    let mut rooms_created: usize = 0;
    let mut rooms_skipped: usize = 0;
    let mut polls_created: usize = 0;

    for seed_room in &content.rooms {
        let room = match create_room(
            pool,
            &seed_room.name,
            Some(&seed_room.description),
            "identity_verified",
        )
        .await
        {
            Ok(r) => {
                rooms_created += 1;
                r
            }
            Err(RoomRepoError::DuplicateName) => {
                tracing::info!(name = %seed_room.name, "room already exists, skipping");
                rooms_skipped += 1;
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        for seed_poll in &seed_room.polls {
            let poll = create_poll(
                pool,
                room.id,
                &seed_poll.question,
                Some(&seed_poll.description),
            )
            .await?;

            for (idx, seed_dim) in seed_poll.dimensions.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let sort_order = idx as i32;
                create_dimension(
                    pool,
                    poll.id,
                    &seed_dim.name,
                    Some(&seed_dim.description),
                    seed_dim.min,
                    seed_dim.max,
                    sort_order,
                )
                .await?;
            }

            update_poll_status(pool, poll.id, "active").await?;
            polls_created += 1;
        }
    }

    Ok(InsertResult {
        rooms_created,
        rooms_skipped,
        polls_created,
    })
}

/// Count rooms that have at least one active poll.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn count_active_rooms(pool: &PgPool) -> Result<usize, anyhow::Error> {
    let row: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(DISTINCT r.id)
        FROM rooms__rooms r
        INNER JOIN rooms__polls p ON p.room_id = r.id
        WHERE r.status = 'open' AND p.status = 'active'
        ",
    )
    .fetch_one(pool)
    .await?;

    let count = usize::try_from(row.0).unwrap_or(0);
    Ok(count)
}

#[derive(sqlx::FromRow)]
struct PollDimensionRow {
    poll_id: Uuid,
    dimension_id: Uuid,
    min_value: f32,
    max_value: f32,
}

/// Get all active polls with their dimensions.
///
/// Returns a list of `(poll_id, dimensions)` where each dimension is
/// `(dimension_id, min_value, max_value)`, ordered by sort order.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn list_active_polls_with_dimensions(
    pool: &PgPool,
) -> Result<Vec<PollWithDimensions>, anyhow::Error> {
    let rows: Vec<PollDimensionRow> = sqlx::query_as(
        r"
        SELECT p.id AS poll_id, d.id AS dimension_id, d.min_value, d.max_value
        FROM rooms__polls p
        INNER JOIN rooms__poll_dimensions d ON d.poll_id = p.id
        WHERE p.status = 'active'
        ORDER BY p.id, d.sort_order
        ",
    )
    .fetch_all(pool)
    .await?;

    let mut result: Vec<PollWithDimensions> = Vec::new();

    for row in rows {
        if let Some(last) = result.last_mut() {
            if last.0 == row.poll_id {
                last.1
                    .push((row.dimension_id, row.min_value, row.max_value));
                continue;
            }
        }
        result.push((
            row.poll_id,
            vec![(row.dimension_id, row.min_value, row.max_value)],
        ));
    }

    Ok(result)
}
