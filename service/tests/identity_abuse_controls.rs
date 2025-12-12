#![allow(clippy::float_cmp)]

use tinycongress_api::db;
use tinycongress_api::identity::abuse::audit::AuditEvent;
use tinycongress_api::identity::abuse::rate_limit::{
    check_rate_limit, cleanup_old_rate_limits, increment_rate_limit, RateLimitConfig,
    RateLimitError,
};
use uuid::Uuid;

#[tokio::test]
async fn test_rate_limit_account_limit() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, endorsement_rate_limits CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let config = RateLimitConfig {
        max_per_account_per_day: 3,
        max_per_subject_topic_per_day: 10,
        window_hours: 24,
    };

    // First 3 endorsements should succeed
    for i in 0..3 {
        let result = check_rate_limit(
            &pool,
            account_id,
            "account",
            &format!("subject{i}"),
            &format!("topic{i}"),
            &config,
        )
        .await;
        assert!(result.is_ok());

        increment_rate_limit(
            &pool,
            account_id,
            "account",
            &format!("subject{i}"),
            &format!("topic{i}"),
        )
        .await
        .unwrap();
    }

    // 4th endorsement should fail with account limit
    let result = check_rate_limit(
        &pool,
        account_id,
        "account",
        "subject3",
        "topic3",
        &config,
    )
    .await;

    assert!(matches!(result, Err(RateLimitError::AccountLimit(3, 24))));
}

#[tokio::test]
async fn test_rate_limit_subject_topic_limit() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, endorsement_rate_limits CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let config = RateLimitConfig {
        max_per_account_per_day: 50,
        max_per_subject_topic_per_day: 2,
        window_hours: 24,
    };

    let subject_id = "same_subject";
    let topic = "same_topic";

    // First 2 endorsements to same subject/topic should succeed
    for _ in 0..2 {
        let result = check_rate_limit(&pool, account_id, "account", subject_id, topic, &config).await;
        assert!(result.is_ok());

        increment_rate_limit(&pool, account_id, "account", subject_id, topic)
            .await
            .unwrap();
    }

    // 3rd endorsement to same subject/topic should fail
    let result = check_rate_limit(&pool, account_id, "account", subject_id, topic, &config).await;

    match result {
        Err(RateLimitError::SubjectTopicLimit { count, .. }) => {
            assert_eq!(count, 2);
        }
        _ => panic!("Expected SubjectTopicLimit error"),
    }
}

#[tokio::test]
async fn test_rate_limit_different_subjects_ok() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, endorsement_rate_limits CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let config = RateLimitConfig {
        max_per_account_per_day: 50,
        max_per_subject_topic_per_day: 2,
        window_hours: 24,
    };

    // Different subjects should not interfere with each other
    for i in 0..5 {
        let result = check_rate_limit(
            &pool,
            account_id,
            "account",
            &format!("subject{i}"),
            "topic",
            &config,
        )
        .await;
        assert!(result.is_ok());

        increment_rate_limit(&pool, account_id, "account", &format!("subject{i}"), "topic")
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn test_cleanup_old_rate_limits() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, endorsement_rate_limits CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert a rate limit entry from 10 days ago
    sqlx::query!(
        r#"
        INSERT INTO endorsement_rate_limits (account_id, subject_type, subject_id, topic, window_start, count)
        VALUES ($1, 'account', 'subject', 'topic', NOW() - INTERVAL '10 days', 5)
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert a recent entry
    increment_rate_limit(&pool, account_id, "account", "subject2", "topic")
        .await
        .unwrap();

    // Cleanup entries older than 7 days
    let deleted = cleanup_old_rate_limits(&pool, 7).await.unwrap();
    assert_eq!(deleted, 1);

    // Verify only recent entry remains
    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::int as "count!"
        FROM endorsement_rate_limits
        WHERE account_id = $1
        "#,
        account_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(count, 1);
}

#[test]
fn test_audit_event_serialization() {
    let event = AuditEvent::EndorsementWrite {
        account_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        subject_type: "account".to_string(),
        subject_id: "test".to_string(),
        topic: "is_real_person".to_string(),
        magnitude: 1.0,
        confidence: 0.9,
    };

    let json = serde_json::to_string(&event).expect("Should serialize");
    assert!(json.contains("endorsement_write"));
    assert!(json.contains("is_real_person"));
}

#[test]
fn test_rate_limit_error_display() {
    let err = RateLimitError::AccountLimit(50, 24);
    assert!(err.to_string().contains("50"));
    assert!(err.to_string().contains("24"));

    let err = RateLimitError::SubjectTopicLimit {
        subject_type: "account".to_string(),
        subject_id: "test".to_string(),
        topic: "topic1".to_string(),
        count: 10,
        window_hours: 24,
    };
    assert!(err.to_string().contains("topic1"));
    assert!(err.to_string().contains("10"));
}
