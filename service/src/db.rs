use sqlx_core::{
    error::Error as SqlxError, migrate::Migrator, query::query, query_scalar::query_scalar,
    row::Row,
};
use sqlx_postgres::{PgPool, PgPoolOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn};

/// Connect to the database and run migrations.
///
/// # Errors
/// Returns an error if the database connection cannot be established or
/// migrations fail to run.
pub async fn setup_database(database_url: &str) -> Result<PgPool, anyhow::Error> {
    let retry_deadline = Duration::from_secs(60); // overall retry budget
    let max_interval = Duration::from_secs(30); // cap single waits
    let mut delay = Duration::from_millis(500);
    let start = Instant::now();

    let pool = loop {
        info!("Attempting to connect to Postgres...");

        match PgPoolOptions::new()
            .max_connections(10)
            // Allow extra time to acquire a connection during startup bursts
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await
        {
            Ok(pool) => break pool,
            Err(err) => {
                if start.elapsed() >= retry_deadline {
                    warn!(error = %err, "Postgres not ready; retries exhausted");
                    return Err(err.into());
                }

                warn!(error = %err, "Postgres not ready yet; retrying");
                sleep(delay).await;
                delay = (delay.saturating_mul(2)).min(max_interval);
            }
        }
    };

    // Resolve the migrations directory in a way that works in release images too.
    // Preference order:
    //  1. MIGRATIONS_DIR env var (allows containers to mount migrations elsewhere)
    //  2. ./migrations relative to the running binary
    //  3. The compile-time manifest directory for local `cargo run`
    let candidate_dirs = [
        std::env::var_os("MIGRATIONS_DIR").map(PathBuf::from),
        Some(PathBuf::from("./migrations")),
        Some(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations"
        ))),
    ];

    let mut last_error = None;
    let mut migrator = None;

    for dir in candidate_dirs.into_iter().flatten() {
        match Migrator::new(Path::new(&dir)).await {
            Ok(found) => {
                info!("Using migrations from {}", dir.display());
                migrator = Some(found);
                break;
            }
            Err(err) => {
                last_error = Some((dir, err));
            }
        }
    }

    let migrator = migrator.ok_or_else(|| match last_error {
        Some((dir, err)) => {
            anyhow::anyhow!("failed to load migrations from {}: {}", dir.display(), err)
        }
        None => anyhow::anyhow!("failed to resolve migrations directory"),
    })?;

    migrator.run(&pool).await?;
    info!("Migrations applied");
    Ok(pool)
}

/// Get the number of active rounds.
///
/// # Errors
/// Returns an error when the query fails to execute.
pub async fn get_active_round_count(pool: &PgPool) -> Result<i64, SqlxError> {
    query_scalar::<_, i64>(
        r"
        SELECT COUNT(*)
        FROM rounds
        WHERE status = 'active'
        ",
    )
    .fetch_one(pool)
    .await
}

/// Create seed data for testing and ensure at least one active round exists.
///
/// # Errors
/// Returns an error when any database call fails.
pub async fn create_seed_data(pool: &PgPool) -> Result<(), SqlxError> {
    // Create some topics if none exist
    let topics_count = query_scalar::<_, i64>("SELECT COUNT(*) FROM topics")
        .fetch_one(pool)
        .await?;

    if topics_count == 0 {
        // Create topics
        let topics = vec![
            ("Healthcare Reform", "Improve access to healthcare"),
            ("Climate Action", "Address climate change with policy"),
            ("Education Funding", "Increase funding for public education"),
            ("Infrastructure", "Rebuild roads, bridges, and utilities"),
            ("Tax Reform", "Simplify tax code and close loopholes"),
        ];

        for (title, description) in topics {
            let topic_id = uuid::Uuid::new_v4();

            query(
                r"
                INSERT INTO topics (id, title, description)
                VALUES ($1, $2, $3)
                ",
            )
            .bind(topic_id)
            .bind(title)
            .bind(description)
            .execute(pool)
            .await?;

            // Add to rankings
            query(
                r"
                INSERT INTO topic_rankings (topic_id, rank, score)
                VALUES ($1, 0, 1500.0)
                ",
            )
            .bind(topic_id)
            .execute(pool)
            .await?;
        }
    }

    // Create active round if none exists
    let active_rounds = get_active_round_count(pool).await?;

    if active_rounds == 0 {
        let round_id = uuid::Uuid::new_v4();
        let now = chrono::Utc::now();
        let end_time = now + chrono::Duration::minutes(10);

        query(
            r"
            INSERT INTO rounds (id, start_time, end_time, status)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(round_id)
        .bind(now)
        .bind(end_time)
        .bind("active")
        .execute(pool)
        .await?;

        // Create pairings for the round
        let topics = query(
            r"
            SELECT id FROM topics
            ORDER BY RANDOM()
            LIMIT 4
            ",
        )
        .fetch_all(pool)
        .await?;

        if topics.len() >= 2 {
            // Create at least one pairing
            let pairing_id = uuid::Uuid::new_v4();
            let topic_a_id: uuid::Uuid = topics[0].try_get("id")?;
            let topic_b_id: uuid::Uuid = topics[1].try_get("id")?;

            query(
                r"
                INSERT INTO pairings (id, round_id, topic_a_id, topic_b_id)
                VALUES ($1, $2, $3, $4)
                ",
            )
            .bind(pairing_id)
            .bind(round_id)
            .bind(topic_a_id)
            .bind(topic_b_id)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}
