use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

/// Connect to the database and run migrations
pub async fn setup_database(database_url: &str) -> Result<PgPool, anyhow::Error> {
    // Create a connection pool with connection timeout and max connections
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(3))
        .connect(database_url)
        .await?;

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
