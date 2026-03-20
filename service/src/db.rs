use crate::config::DatabaseConfig;
use sqlx::Connection;
use sqlx_core::migrate::{MigrateError, Migrator};
use sqlx_postgres::{PgConnection, PgPool, PgPoolOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn};

/// Connect to the database and run migrations.
///
/// Retries the connection with exponential backoff (500ms to 5s) for up to
/// 120 seconds. The deadline is well within the Kubernetes startup probe
/// budget (600s), so the process crashes fast enough for K8s to restart the
/// pod without Helm timing out.
///
/// When `auto_reset_on_migration_failure` is enabled and migrations fail due
/// to a version mismatch (not an SQL execution error), the database is dropped
/// and recreated, then migrations are retried exactly once.
///
/// # Errors
/// Returns an error if the connection cannot be established within the retry
/// deadline, or if migrations fail after a successful connection.
pub async fn setup_database(config: &DatabaseConfig) -> Result<PgPool, anyhow::Error> {
    let pool = connect_with_retry(config).await?;
    let migrator = resolve_migrator(config).await?;

    match migrator.run(&pool).await {
        Ok(()) => {
            info!("Migrations applied");
            Ok(pool)
        }
        Err(err) if config.auto_reset_on_migration_failure && is_resettable_error(&err) => {
            warn!(
                error = %err,
                "Migration failed with resettable error; resetting database"
            );
            pool.close().await;
            reset_database(config).await?;
            let pool = connect_with_retry(config).await?;
            migrator.run(&pool).await?;
            info!("Migrations applied after database reset");
            Ok(pool)
        }
        Err(err) => Err(err.into()),
    }
}

/// Connect to Postgres with exponential backoff, retrying for up to 120 s.
async fn connect_with_retry(config: &DatabaseConfig) -> Result<PgPool, anyhow::Error> {
    let retry_deadline = Duration::from_secs(120);
    let max_interval = Duration::from_secs(5);
    let mut delay = Duration::from_millis(500);
    let start = Instant::now();

    loop {
        info!("Attempting to connect to Postgres...");

        match PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(config.connect_options())
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(err) => {
                if start.elapsed() >= retry_deadline {
                    warn!(error = %err, "Postgres not ready; retries exhausted after {:?}", retry_deadline);
                    return Err(err.into());
                }
                warn!(error = %err, "Postgres not ready yet; retrying in {:?}", delay);
                sleep(delay).await;
                delay = (delay.saturating_mul(2)).min(max_interval);
            }
        }
    }
}

/// Resolve the migrations directory, trying multiple candidate paths.
///
/// Preference order:
///  1. `config.migrations_dir` (from config file or `TC_DATABASE__MIGRATIONS_DIR` env)
///  2. `./migrations` relative to the running binary
///  3. The compile-time manifest directory for local `cargo run`
async fn resolve_migrator(config: &DatabaseConfig) -> Result<Migrator, anyhow::Error> {
    let candidate_dirs = [
        config.migrations_dir.as_ref().map(PathBuf::from),
        Some(PathBuf::from("./migrations")),
        Some(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations"
        ))),
    ];

    let mut last_error = None;

    for dir in candidate_dirs.into_iter().flatten() {
        match Migrator::new(Path::new(&dir)).await {
            Ok(found) => {
                info!("Using migrations from {}", dir.display());
                return Ok(found);
            }
            Err(err) => {
                last_error = Some((dir, err));
            }
        }
    }

    match last_error {
        Some((dir, err)) => Err(anyhow::anyhow!(
            "failed to load migrations from {}: {}",
            dir.display(),
            err
        )),
        None => Err(anyhow::anyhow!("failed to resolve migrations directory")),
    }
}

/// Returns true for migration errors caused by version history divergence,
/// which are fixed by a fresh database. Returns false for SQL execution
/// errors which would recur on a clean DB.
///
/// `MigrateError` is `#[non_exhaustive]`, so the wildcard arm is required.
const fn is_resettable_error(err: &MigrateError) -> bool {
    matches!(
        err,
        MigrateError::VersionMissing(_) | MigrateError::VersionMismatch(_) | MigrateError::Dirty(_)
    )
}

/// Drop and recreate the application database.
///
/// Connects to the `postgres` system database, terminates existing connections
/// to the app database, then drops and recreates it. All operations are logged
/// at `warn!` level since this is a destructive recovery path.
async fn reset_database(config: &DatabaseConfig) -> Result<(), anyhow::Error> {
    let db_name = &config.name;
    warn!(database = %db_name, "Resetting database: terminating connections");

    let mut conn: PgConnection = PgConnection::connect_with(&config.system_connect_options())
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to system database: {e}"))?;

    // Terminate all other connections to the target database.
    sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()"
    )
    .bind(db_name)
    .execute(&mut conn)
    .await
    .map_err(|e| anyhow::anyhow!("failed to terminate connections to {db_name}: {e}"))?;

    // Database identifiers cannot be parameterized — use quoting to prevent
    // injection. The name comes from our own config, not user input.
    //
    // TEMPLATE template0 avoids collation version mismatches that break
    // CREATE DATABASE after a major Postgres upgrade (e.g. PG 17→18).
    // template1 inherits stale collation metadata from the old data dir;
    // template0 is always safe.
    let drop_sql = format!("DROP DATABASE IF EXISTS \"{db_name}\"");
    let create_sql = format!("CREATE DATABASE \"{db_name}\" TEMPLATE template0");

    warn!(database = %db_name, "Dropping database");
    sqlx::query(&drop_sql)
        .execute(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("failed to drop database {db_name}: {e}"))?;

    warn!(database = %db_name, "Creating database");
    sqlx::query(&create_sql)
        .execute(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create database {db_name}: {e}"))?;

    warn!(database = %db_name, "Database reset complete");
    Ok(())
}
