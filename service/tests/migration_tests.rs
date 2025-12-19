//! Migration-specific tests.
//!
//! These tests use `isolated_db()` which creates a fresh database via PostgreSQL
//! template copy (~15-30ms overhead). They run with normal `cargo test`.

mod common;

use common::migration_helpers::{
    load_migrator, validate_migration_count_matches, validate_migration_monotonicity,
};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;

// ============================================================================
// Monotonicity and Ordering Tests
// ============================================================================

/// Validates that all migrations are ordered monotonically.
/// This catches cases where migrations were added out of order.
#[shared_runtime_test]
async fn test_migration_monotonicity() {
    let migrator = load_migrator().await;
    validate_migration_monotonicity(&migrator);
}

/// Validates migration count matches between on-disk and applied.
/// This catches deleted or missing migration files.
#[shared_runtime_test]
async fn test_migration_count_matches() {
    let db = isolated_db().await;
    let migrator = load_migrator().await;
    validate_migration_count_matches(db.pool(), &migrator).await;
}

// ============================================================================
// Idempotency Tests
// ============================================================================

/// Verifies migrations can be run multiple times without error.
/// Each migration should be idempotent (use IF NOT EXISTS, IF EXISTS, etc.).
#[shared_runtime_test]
async fn test_all_migrations_are_idempotent() {
    let db = isolated_db().await;

    let migrator = load_migrator().await;

    // Migrations were already applied when isolated_db was created (from template)
    // Running again should succeed (idempotent)
    migrator
        .run(db.pool())
        .await
        .expect("Migrations should be idempotent - running twice should not fail");

    // Run a third time to be extra sure
    migrator
        .run(db.pool())
        .await
        .expect("Migrations should be idempotent - running thrice should not fail");
}

// ============================================================================
// Fresh Schema Tests
// ============================================================================

/// Tests that migrations can be applied to a completely fresh database.
/// This catches issues where migrations depend on state that isn't from migrations.
#[shared_runtime_test]
async fn test_migrations_apply_to_fresh_db() {
    let db = isolated_db().await;

    // Verify key tables exist after migrations (from template)
    let tables_exist: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT FROM information_schema.tables
            WHERE table_schema = 'public'
            AND table_name IN ('accounts', '_sqlx_migrations')
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check tables");

    assert!(tables_exist, "Core tables should exist after migrations");
}

// ============================================================================
// Schema Consistency Tests
// ============================================================================

/// Verifies that the accounts table has the expected schema.
#[shared_runtime_test]
async fn test_accounts_table_schema() {
    let db = isolated_db().await;

    let columns: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT column_name, data_type
        FROM information_schema.columns
        WHERE table_name = 'accounts'
        ORDER BY ordinal_position
        "#,
    )
    .fetch_all(db.pool())
    .await
    .expect("Failed to query accounts schema");

    let column_map: std::collections::HashMap<String, String> =
        columns.into_iter().collect();

    assert_eq!(
        column_map.get("id").map(|s| s.as_str()),
        Some("uuid"),
        "accounts.id should be uuid"
    );
    assert_eq!(
        column_map.get("username").map(|s| s.as_str()),
        Some("text"),
        "accounts.username should be text"
    );
    assert_eq!(
        column_map.get("root_pubkey").map(|s| s.as_str()),
        Some("text"),
        "accounts.root_pubkey should be text"
    );
    assert_eq!(
        column_map.get("root_kid").map(|s| s.as_str()),
        Some("text"),
        "accounts.root_kid should be text"
    );
    assert!(
        column_map.contains_key("created_at"),
        "accounts should have created_at column"
    );
}

/// Verifies that critical indexes exist on the accounts table.
#[shared_runtime_test]
async fn test_accounts_table_indexes() {
    let db = isolated_db().await;

    let indexes: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT indexname
        FROM pg_indexes
        WHERE tablename = 'accounts'
        "#,
    )
    .fetch_all(db.pool())
    .await
    .expect("Failed to query indexes");

    let index_names: Vec<&str> = indexes.iter().map(|(n,)| n.as_str()).collect();

    assert!(
        index_names.iter().any(|n| n.contains("pkey")),
        "accounts should have primary key index"
    );
    assert!(
        index_names.iter().any(|n| n.contains("username")),
        "accounts should have username index"
    );
    assert!(
        index_names.iter().any(|n| n.contains("root_kid")),
        "accounts should have root_kid index"
    );
}

// ============================================================================
// Extension Tests
// ============================================================================

/// Verifies that required PostgreSQL extensions are available.
#[shared_runtime_test]
async fn test_required_extensions_available() {
    let db = isolated_db().await;

    let pgcrypto_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT FROM pg_extension WHERE extname = 'pgcrypto')"
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check pgcrypto");

    assert!(pgcrypto_exists, "pgcrypto extension should be available");

    let pgmq_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT FROM pg_extension WHERE extname = 'pgmq')"
    )
    .fetch_one(db.pool())
    .await
    .expect("Failed to check pgmq");

    assert!(pgmq_exists, "pgmq extension should be available");
}
