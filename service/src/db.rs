use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use tracing::{info, warn};

/// Connect to the database and run migrations
pub async fn setup_database(database_url: &str) -> Result<PgPool, anyhow::Error> {
    // Use backoff for cleaner retry with jitter and time budget
    use backoff::{future::retry, Error as BackoffError, ExponentialBackoff};

    let backoff = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(60)), // overall retry budget
        max_interval: Duration::from_secs(30),            // cap single waits
        ..ExponentialBackoff::default()
    };

    let pool = retry(backoff, || async {
        info!("Attempting to connect to Postgres...");
        match PgPoolOptions::new()
            .max_connections(10)
            // Allow extra time to acquire a connection during startup bursts
            .acquire_timeout(Duration::from_secs(30))
            .connect(database_url)
            .await
        {
            Ok(pool) => Ok(pool),
            Err(e) => {
                warn!(error = %e, "Postgres not ready yet; retrying");
                Err(BackoffError::transient(e))
            }
        }
    })
    .await?;

    // Run database migrations (embedded at compile time)
    sqlx::migrate!().run(&pool).await?;
    info!("Migrations applied");
    Ok(pool)
}

// Get the number of active rounds
pub async fn get_active_round_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM rounds
        WHERE status = 'active'
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(result.count.unwrap_or(0))
}

// Create seed data for testing
pub async fn create_seed_data(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Create some topics if none exist
    let topics_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM topics"
    )
    .fetch_one(pool)
    .await?
    .count
    .unwrap_or(0);

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

            sqlx::query!(
                r#"
                INSERT INTO topics (id, title, description)
                VALUES ($1, $2, $3)
                "#,
                topic_id,
                title,
                description
            )
            .execute(pool)
            .await?;

            // Add to rankings
            sqlx::query!(
                r#"
                INSERT INTO topic_rankings (topic_id, rank, score)
                VALUES ($1, 0, 1500.0)
                "#,
                topic_id
            )
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

        sqlx::query!(
            r#"
            INSERT INTO rounds (id, start_time, end_time, status)
            VALUES ($1, $2, $3, $4)
            "#,
            round_id,
            now,
            end_time,
            "active"
        )
        .execute(pool)
        .await?;

        // Create pairings for the round
        let topics = sqlx::query!(
            r#"
            SELECT id FROM topics
            ORDER BY RANDOM()
            LIMIT 4
            "#
        )
        .fetch_all(pool)
        .await?;

        if topics.len() >= 2 {
            // Create at least one pairing
            let pairing_id = uuid::Uuid::new_v4();

            sqlx::query!(
                r#"
                INSERT INTO pairings (id, round_id, topic_a_id, topic_b_id)
                VALUES ($1, $2, $3, $4)
                "#,
                pairing_id,
                round_id,
                topics[0].id,
                topics[1].id
            )
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}
