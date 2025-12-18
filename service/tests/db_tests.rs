//! Database integration tests using testcontainers.
//!
//! These tests use a shared Tokio runtime to ensure proper async cleanup.
//! See common/mod.rs for details on why this pattern is necessary.

mod common;

use common::test_db::{get_test_db, run_test, test_transaction};
use sqlx::{query, query_scalar};
use uuid::Uuid;

/// Test that we can connect to the database and run queries.
#[test]
fn test_db_connection() {
    run_test(async {
        let db = get_test_db().await;

        // Simple connectivity test
        let result: i32 = query_scalar("SELECT 1")
            .fetch_one(db.pool())
            .await
            .expect("Failed to execute query");

        assert_eq!(result, 1);
    });
}

/// Test that migrations ran successfully by checking for our test table.
#[test]
fn test_migrations_applied() {
    run_test(async {
        let db = get_test_db().await;

        // Check that the test_items table exists
        let exists: bool = query_scalar(
            r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_name = 'test_items'
            )
            "#,
        )
        .fetch_one(db.pool())
        .await
        .expect("Failed to check table existence");

        assert!(exists, "test_items table should exist after migrations");
    });
}

/// Test basic CRUD operations.
#[test]
fn test_crud_operations() {
    run_test(async {
        let mut tx = test_transaction().await;

        // Insert a test item
        let item_id = Uuid::new_v4();
        let item_name = format!("Test Item {}", item_id);

        query("INSERT INTO test_items (id, name) VALUES ($1, $2)")
            .bind(item_id)
            .bind(&item_name)
            .execute(&mut *tx)
            .await
            .expect("Failed to insert test item");

        // Verify the item exists
        let count: i64 = query_scalar("SELECT COUNT(*) FROM test_items WHERE id = $1")
            .bind(item_id)
            .fetch_one(&mut *tx)
            .await
            .expect("Failed to count items");

        assert_eq!(count, 1, "Should find the inserted item");
    });
}

/// Test that pgmq extension is available (from custom postgres image).
#[test]
fn test_pgmq_extension_available() {
    run_test(async {
        let db = get_test_db().await;

        // Check that pgmq extension exists
        let exists: bool = query_scalar(
            r#"
            SELECT EXISTS (
                SELECT FROM pg_extension WHERE extname = 'pgmq'
            )
            "#,
        )
        .fetch_one(db.pool())
        .await
        .expect("Failed to check pgmq extension");

        assert!(exists, "pgmq extension should be available");
    });
}
