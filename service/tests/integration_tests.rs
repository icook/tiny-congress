use sqlx::{PgPool, Executor};
use tinycongress_api::db;
use uuid::Uuid;


async fn get_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());

    // Use the application's database setup function
    let pool = db::setup_database(&database_url).await.unwrap();

    // Additional clean-up specific to tests
    let mut conn = pool.acquire().await.unwrap();

    // Make sure we're starting with empty tables
    conn.execute("TRUNCATE TABLE votes CASCADE").await.unwrap();
    conn.execute("TRUNCATE TABLE pairings CASCADE").await.unwrap();
    conn.execute("TRUNCATE TABLE topic_rankings CASCADE").await.unwrap();
    conn.execute("TRUNCATE TABLE rounds CASCADE").await.unwrap();
    conn.execute("TRUNCATE TABLE topics CASCADE").await.unwrap();

    pool
}

// Helper to insert a test topic
async fn insert_test_topic(pool: &PgPool, title: &str, description: &str) -> Uuid {
    let topic_id = Uuid::new_v4();

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
    .await
    .unwrap();

    // Insert into rankings as well
    sqlx::query!(
        r#"
        INSERT INTO topic_rankings (topic_id, rank, score)
        VALUES ($1, 0, 1500.0)
        "#,
        topic_id
    )
    .execute(pool)
    .await
    .unwrap();

    topic_id
}

// Helper to create a test round
async fn insert_test_round(pool: &PgPool, status: &str) -> Uuid {
    let round_id = Uuid::new_v4();
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
        status
    )
    .execute(pool)
    .await
    .unwrap();

    round_id
}

// Helper to create a test pairing
async fn insert_test_pairing(pool: &PgPool, round_id: Uuid, topic_a_id: Uuid, topic_b_id: Uuid) -> Uuid {
    let pairing_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO pairings (id, round_id, topic_a_id, topic_b_id)
        VALUES ($1, $2, $3, $4)
        "#,
        pairing_id,
        round_id,
        topic_a_id,
        topic_b_id
    )
    .execute(pool)
    .await
    .unwrap();

    pairing_id
}

// Helper to submit a vote
async fn insert_test_vote(pool: &PgPool, pairing_id: Uuid, user_id: &str, choice_id: Uuid) -> Uuid {
    let vote_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO votes (id, pairing_id, user_id, choice_id)
        VALUES ($1, $2, $3, $4)
        "#,
        vote_id,
        pairing_id,
        user_id,
        choice_id
    )
    .execute(pool)
    .await
    .unwrap();

    vote_id
}

#[tokio::test]
async fn test_topic_crud() {
    let pool = get_pool().await;

    // Insert a topic
    let title = "Test Topic";
    let description = "This is a test topic";
    let topic_id = insert_test_topic(&pool, title, description).await;

    // Retrieve the topic
    let topic = sqlx::query!(
        r#"
        SELECT id, title, description
        FROM topics
        WHERE id = $1
        "#,
        topic_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Verify data
    assert_eq!(topic.title, title);
    assert_eq!(topic.description, description);

    // Update the topic
    let new_title = "Updated Topic";
    sqlx::query!(
        r#"
        UPDATE topics
        SET title = $1
        WHERE id = $2
        "#,
        new_title,
        topic_id
    )
    .execute(&pool)
    .await
    .unwrap();

    // Verify update
    let updated_topic = sqlx::query!(
        r#"
        SELECT title
        FROM topics
        WHERE id = $1
        "#,
        topic_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(updated_topic.title, new_title);

    // Delete the topic
    sqlx::query!(
        r#"
        DELETE FROM topic_rankings
        WHERE topic_id = $1
        "#,
        topic_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        DELETE FROM topics
        WHERE id = $1
        "#,
        topic_id
    )
    .execute(&pool)
    .await
    .unwrap();

    // Verify deletion
    let count = sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM topics
        WHERE id = $1
        "#,
        topic_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .count
    .unwrap_or(0);

    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_round_pairing_and_voting() {
    let pool = get_pool().await;

    // Create two topics
    let topic_a_id = insert_test_topic(&pool, "Topic A", "First topic").await;
    let topic_b_id = insert_test_topic(&pool, "Topic B", "Second topic").await;

    // Create an active round
    let round_id = insert_test_round(&pool, "active").await;

    // Create a pairing
    let pairing_id = insert_test_pairing(&pool, round_id, topic_a_id, topic_b_id).await;

    // Submit votes
    let user1 = "user1";
    let user2 = "user2";
    let user3 = "user3";

    // Two votes for topic A, one for topic B
    insert_test_vote(&pool, pairing_id, user1, topic_a_id).await;
    insert_test_vote(&pool, pairing_id, user2, topic_a_id).await;
    insert_test_vote(&pool, pairing_id, user3, topic_b_id).await;

    // Count votes
    let votes_for_a = sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM votes
        WHERE pairing_id = $1 AND choice_id = $2
        "#,
        pairing_id,
        topic_a_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .count
    .unwrap_or(0);

    let votes_for_b = sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM votes
        WHERE pairing_id = $1 AND choice_id = $2
        "#,
        pairing_id,
        topic_b_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .count
    .unwrap_or(0);

    // Verify vote counts
    assert_eq!(votes_for_a, 2);
    assert_eq!(votes_for_b, 1);

    // Update rankings based on the votes
    // Simple ELO update: winner gets +15, loser gets -15
    let current_score_a = sqlx::query!(
        r#"
        SELECT score
        FROM topic_rankings
        WHERE topic_id = $1
        "#,
        topic_a_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .score;

    let current_score_b = sqlx::query!(
        r#"
        SELECT score
        FROM topic_rankings
        WHERE topic_id = $1
        "#,
        topic_b_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .score;

    // Topic A won, update scores
    sqlx::query!(
        r#"
        UPDATE topic_rankings
        SET score = $1, updated_at = NOW()
        WHERE topic_id = $2
        "#,
        current_score_a + 15.0,
        topic_a_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        UPDATE topic_rankings
        SET score = $1, updated_at = NOW()
        WHERE topic_id = $2
        "#,
        current_score_b - 15.0,
        topic_b_id
    )
    .execute(&pool)
    .await
    .unwrap();

    // Verify updated scores
    let updated_score_a = sqlx::query!(
        r#"
        SELECT score
        FROM topic_rankings
        WHERE topic_id = $1
        "#,
        topic_a_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .score;

    let updated_score_b = sqlx::query!(
        r#"
        SELECT score
        FROM topic_rankings
        WHERE topic_id = $1
        "#,
        topic_b_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .score;

    assert_eq!(updated_score_a, current_score_a + 15.0);
    assert_eq!(updated_score_b, current_score_b - 15.0);

    // Update the ranks based on scores
    sqlx::query!(
        r#"
        WITH ranked_topics AS (
            SELECT
                topic_id,
                score,
                RANK() OVER (ORDER BY score DESC) as new_rank
            FROM topic_rankings
        )
        UPDATE topic_rankings tr
        SET rank = rt.new_rank
        FROM ranked_topics rt
        WHERE tr.topic_id = rt.topic_id
        "#
    )
    .execute(&pool)
    .await
    .unwrap();

    // Verify ranks
    let ranks = sqlx::query!(
        r#"
        SELECT topic_id, rank, score
        FROM topic_rankings
        ORDER BY rank ASC
        "#
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // Check if Topic A has rank 1 (it won)
    let topic_a_rank = ranks.iter()
        .find(|r| r.topic_id == topic_a_id)
        .map(|r| r.rank)
        .unwrap_or(0);

    let topic_b_rank = ranks.iter()
        .find(|r| r.topic_id == topic_b_id)
        .map(|r| r.rank)
        .unwrap_or(0);

    assert_eq!(topic_a_rank, 1);
    assert_eq!(topic_b_rank, 2);
}

#[tokio::test]
async fn test_top_topics() {
    let pool = get_pool().await;

    // Create multiple topics with different scores
    let topic_ids = vec![
        insert_test_topic(&pool, "Topic 1", "First topic").await,
        insert_test_topic(&pool, "Topic 2", "Second topic").await,
        insert_test_topic(&pool, "Topic 3", "Third topic").await,
        insert_test_topic(&pool, "Topic 4", "Fourth topic").await,
        insert_test_topic(&pool, "Topic 5", "Fifth topic").await,
    ];

    // Set different scores
    let scores = vec![1600.0, 1550.0, 1525.0, 1450.0, 1400.0];

    for (i, topic_id) in topic_ids.iter().enumerate() {
        sqlx::query!(
            r#"
            UPDATE topic_rankings
            SET score = $1, updated_at = NOW()
            WHERE topic_id = $2
            "#,
            scores[i],
            topic_id
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Update ranks
    sqlx::query!(
        r#"
        WITH ranked_topics AS (
            SELECT
                topic_id,
                score,
                RANK() OVER (ORDER BY score DESC) as new_rank
            FROM topic_rankings
        )
        UPDATE topic_rankings tr
        SET rank = rt.new_rank
        FROM ranked_topics rt
        WHERE tr.topic_id = rt.topic_id
        "#
    )
    .execute(&pool)
    .await
    .unwrap();

    // Get top 3 topics
    let top_topics = sqlx::query!(
        r#"
        SELECT tr.topic_id, tr.rank, tr.score, t.title
        FROM topic_rankings tr
        JOIN topics t ON tr.topic_id = t.id
        ORDER BY tr.score DESC
        LIMIT 3
        "#
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // Verify we got 3 topics
    assert_eq!(top_topics.len(), 3);

    // Verify order
    assert_eq!(top_topics[0].rank, 1);
    assert_eq!(top_topics[1].rank, 2);
    assert_eq!(top_topics[2].rank, 3);

    // Verify scores are in descending order
    assert!(top_topics[0].score > top_topics[1].score);
    assert!(top_topics[1].score > top_topics[2].score);
}

#[tokio::test]
async fn test_active_round() {
    let pool = get_pool().await;

    // Clear any existing rounds
    sqlx::query!("DELETE FROM rounds").execute(&pool).await.unwrap();

    // Create a completed round
    let _completed_round_id = insert_test_round(&pool, "completed").await;

    // Create an active round
    let active_round_id = insert_test_round(&pool, "active").await;

    // Query for the active round
    let active_round = sqlx::query!(
        r#"
        SELECT id, status
        FROM rounds
        WHERE status = 'active'
        LIMIT 1
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Verify we got the active round
    assert_eq!(active_round.id, active_round_id);
    assert_eq!(active_round.status, "active");
}
