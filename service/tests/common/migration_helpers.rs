//! Migration testing utilities.
//!
//! These helpers validate migration ordering, monotonicity, and schema consistency.

use sqlx::PgPool;
use sqlx_core::migrate::{Migration, Migrator};
use std::collections::HashSet;
use std::path::Path;

/// Loads the migrator from the standard migrations directory.
#[allow(clippy::expect_used)]
pub async fn load_migrator() -> Migrator {
    let migrations_path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"));
    Migrator::new(migrations_path)
        .await
        .expect("Failed to load migrations from migrations/")
}

/// Validates that all migrations are ordered monotonically by version number.
///
/// # Panics
/// Panics with a clear message if migrations are out of order or have duplicate versions.
pub fn validate_migration_monotonicity(migrator: &Migrator) {
    let migrations = migrator.iter();
    let mut versions: Vec<i64> = Vec::new();
    let mut seen_versions: HashSet<i64> = HashSet::new();

    for migration in migrations {
        let version = migration.version;

        // Check for duplicates
        if !seen_versions.insert(version) {
            panic!(
                "MIGRATION ERROR: Duplicate migration version {version}\n\
                 Multiple migrations have version {version}.\n\
                 Each migration must have a unique version number.\n\
                 Check migrations/ for duplicate timestamp prefixes."
            );
        }

        // Check monotonicity
        if let Some(&last_version) = versions.last() {
            if version <= last_version {
                panic!(
                    "MIGRATION ERROR: Migrations are not monotonically ordered\n\
                     Migration {version} comes after {last_version} but has a lower/equal version.\n\
                     Migrations must be ordered by version number (ascending).\n\
                     This usually means migrations were added out of order.\n\
                     Fix: Rename migration files to have proper sequential timestamps."
                );
            }
        }

        versions.push(version);
    }
}

/// Gets the set of migration versions from on-disk migration files.
pub fn get_ondisk_migration_versions(migrator: &Migrator) -> HashSet<i64> {
    migrator.iter().map(|m| m.version).collect()
}

/// Gets the set of applied migration versions from the database.
#[allow(clippy::expect_used)]
pub async fn get_applied_migration_versions(pool: &PgPool) -> HashSet<i64> {
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .expect("Failed to query _sqlx_migrations table");

    rows.into_iter().map(|(v,)| v).collect()
}

/// Validates that applied migrations match on-disk migrations.
///
/// # Panics
/// - If there are migrations in the database that don't exist on disk (deleted migrations)
/// - If there are migrations on disk that haven't been applied (unapplied migrations)
#[allow(clippy::expect_used)]
pub async fn validate_migration_count_matches(pool: &PgPool, migrator: &Migrator) {
    let ondisk_versions = get_ondisk_migration_versions(migrator);
    let applied_versions = get_applied_migration_versions(pool).await;

    // Check for migrations in DB that don't exist on disk
    let deleted: Vec<_> = applied_versions.difference(&ondisk_versions).collect();
    if !deleted.is_empty() {
        panic!(
            "MIGRATION ERROR: Applied migrations not found on disk\n\
             The following migrations are in the database but not in migrations/:\n\
             {deleted:?}\n\
             This usually means migration files were deleted after being applied.\n\
             Fix: Restore the deleted migration files or manually remove from _sqlx_migrations."
        );
    }

    // Check for migrations on disk that haven't been applied
    let unapplied: Vec<_> = ondisk_versions.difference(&applied_versions).collect();
    if !unapplied.is_empty() {
        panic!(
            "MIGRATION ERROR: Unapplied migrations found\n\
             The following migrations exist on disk but are not applied:\n\
             {unapplied:?}\n\
             This is expected for new migrations. Run migrations to apply them."
        );
    }
}

/// Describes a migration for display purposes.
pub fn describe_migration(migration: &Migration) -> String {
    format!(
        "v{}: {} ({})",
        migration.version,
        migration.description,
        if migration.migration_type.is_down_migration() {
            "DOWN"
        } else {
            "UP"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_test_macros::shared_runtime_test;

    #[shared_runtime_test]
    async fn test_load_migrator_succeeds() {
        // This will fail if migrations directory is missing or malformed
        let migrator = load_migrator().await;
        assert!(
            migrator.iter().count() > 0,
            "Should have at least one migration"
        );
    }

    #[shared_runtime_test]
    async fn test_validate_monotonicity_passes_for_valid_migrations() {
        let migrator = load_migrator().await;
        // Should not panic
        validate_migration_monotonicity(&migrator);
    }
}
