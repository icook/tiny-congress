//! Integration tests for trust action log repository operations.

mod common;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::repo::action_queue::ERROR_MESSAGE_MAX_LEN;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo, TrustRepoError};
use tinycongress_api::trust::service::ActionType;
use uuid::Uuid;

#[shared_runtime_test]
async fn test_enqueue_action_creates_pending() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(100)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let payload = serde_json::json!({"target_id": "some-uuid"});
    let record = repo
        .enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue_action");

    assert_eq!(record.actor_id, account.id);
    assert_eq!(record.action_type, "endorse");
    assert_eq!(record.status, "pending");
    assert_eq!(record.payload, payload);
    assert!(record.error_message.is_none());
    assert!(record.processed_at.is_none());
}

#[shared_runtime_test]
async fn test_count_daily_actions() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(101)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let payload = serde_json::json!({});

    repo.enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue 1");
    repo.enqueue_action(account.id, ActionType::Revoke, &payload)
        .await
        .expect("enqueue 2");
    repo.enqueue_action(account.id, ActionType::Denounce, &payload)
        .await
        .expect("enqueue 3");

    let count = repo
        .count_daily_actions(account.id)
        .await
        .expect("count_daily_actions");

    assert_eq!(count, 3);
}

#[shared_runtime_test]
async fn test_get_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(102)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let payload = serde_json::json!({});

    let enqueued = repo
        .enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue");

    let fetched = repo.get_action(enqueued.id).await.expect("get_action");

    assert_eq!(fetched.id, enqueued.id);
    assert_eq!(fetched.actor_id, account.id);
    assert_eq!(fetched.action_type, "endorse");
    assert_eq!(fetched.status, "pending");
}

#[shared_runtime_test]
async fn test_get_action_returns_notfound_for_unknown_id() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let repo = PgTrustRepo::new(pool);
    let result = repo.get_action(Uuid::new_v4()).await;

    assert!(
        matches!(result, Err(TrustRepoError::NotFound)),
        "expected NotFound for unknown action id, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_complete_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(104)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool.clone());
    let payload = serde_json::json!({});

    let record = repo
        .enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue");

    repo.complete_action(record.id)
        .await
        .expect("complete_action");

    // Verify via a direct query that status='completed' and processed_at is set
    let row = sqlx::query_as::<_, (String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT status, processed_at FROM trust__action_log WHERE id = $1",
    )
    .bind(record.id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    assert_eq!(row.0, "completed");
    assert!(row.1.is_some());
}

#[shared_runtime_test]
async fn test_fail_action_with_message() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(105)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let payload = serde_json::json!({});

    let record = repo
        .enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue");

    let error_msg = "target account not found";
    repo.fail_action(record.id, error_msg)
        .await
        .expect("fail_action");

    let row =
        sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
            ),
        >("SELECT status, error_message, processed_at FROM trust__action_log WHERE id = $1")
        .bind(record.id)
        .fetch_one(db.pool())
        .await
        .expect("fetch row");

    assert_eq!(row.0, "failed");
    assert_eq!(row.1.as_deref(), Some(error_msg));
    assert!(row.2.is_some());
}

#[shared_runtime_test]
async fn test_fail_action_truncates_long_error_message() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(106)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let payload = serde_json::json!({});

    let record = repo
        .enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue");

    // Construct an error message that exceeds the cap.
    let long_error: String = "e".repeat(ERROR_MESSAGE_MAX_LEN + 500);
    repo.fail_action(record.id, &long_error)
        .await
        .expect("fail_action");

    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, error_message FROM trust__action_log WHERE id = $1",
    )
    .bind(record.id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    assert_eq!(row.0, "failed");
    let stored = row.1.expect("error_message should be set");
    assert_eq!(
        stored.chars().count(),
        ERROR_MESSAGE_MAX_LEN,
        "stored error message must be truncated to ERROR_MESSAGE_MAX_LEN characters"
    );
}

#[shared_runtime_test]
async fn test_complete_action_returns_notfound_for_unknown_id() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let repo = PgTrustRepo::new(pool);
    let result = repo.complete_action(Uuid::new_v4()).await;

    assert!(
        matches!(result, Err(TrustRepoError::NotFound)),
        "expected NotFound for unknown action id, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_fail_action_returns_notfound_for_unknown_id() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let repo = PgTrustRepo::new(pool);
    let result = repo.fail_action(Uuid::new_v4(), "some error").await;

    assert!(
        matches!(result, Err(TrustRepoError::NotFound)),
        "expected NotFound for unknown action id, got: {result:?}"
    );
}

/// Verify that `fail_action` truncates at a character boundary for multibyte strings.
///
/// The truncation code uses `char_indices().nth(ERROR_MESSAGE_MAX_LEN)` which is
/// character-count-based, not byte-count-based. A string of multibyte characters
/// (e.g. CJK ideographs, 3 bytes each) must still be truncated to exactly
/// ERROR_MESSAGE_MAX_LEN characters, not ERROR_MESSAGE_MAX_LEN bytes — and the
/// slice must not fall in the middle of a code point.
#[shared_runtime_test]
async fn test_fail_action_truncates_multibyte_error_message_at_char_boundary() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(107)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let payload = serde_json::json!({});

    let record = repo
        .enqueue_action(account.id, ActionType::Endorse, &payload)
        .await
        .expect("enqueue");

    // Each '中' is 3 bytes; ERROR_MESSAGE_MAX_LEN + 100 characters = well over
    // ERROR_MESSAGE_MAX_LEN bytes. The truncation must produce exactly
    // ERROR_MESSAGE_MAX_LEN characters (not bytes) without splitting a code point.
    let long_error: String = "中".repeat(ERROR_MESSAGE_MAX_LEN + 100);
    repo.fail_action(record.id, &long_error)
        .await
        .expect("fail_action");

    let row = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, error_message FROM trust__action_log WHERE id = $1",
    )
    .bind(record.id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    assert_eq!(row.0, "failed");
    let stored = row.1.expect("error_message should be set");
    assert_eq!(
        stored.chars().count(),
        ERROR_MESSAGE_MAX_LEN,
        "multibyte truncation must produce exactly ERROR_MESSAGE_MAX_LEN characters"
    );
    // Byte length should be 3× char count for pure CJK input.
    assert_eq!(
        stored.len(),
        ERROR_MESSAGE_MAX_LEN * 3,
        "each '中' is 3 bytes; byte length must equal 3 * char count"
    );
}
