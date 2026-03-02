use crate::config::DatabaseConfig;
use sqlx_core::migrate::Migrator;
use sqlx_postgres::{PgPool, PgPoolOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

/// Connect to the database and run migrations.
///
/// Retries the connection indefinitely with exponential backoff (500ms to 5s).
/// In Kubernetes, the startup probe acts as the effective timeout — if postgres
/// never becomes available, the probe fails and the pod is restarted.
///
/// # Errors
/// Returns an error if migrations fail after a successful connection.
pub async fn setup_database(config: &DatabaseConfig) -> Result<PgPool, anyhow::Error> {
    let max_interval = Duration::from_secs(5);
    let mut delay = Duration::from_millis(500);

    let pool = loop {
        info!("Attempting to connect to Postgres...");

        match PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(Duration::from_secs(30))
            .connect_with(config.connect_options())
            .await
        {
            Ok(pool) => break pool,
            Err(err) => {
                warn!(error = %err, "Postgres not ready yet; retrying in {:?}", delay);
                sleep(delay).await;
                delay = (delay.saturating_mul(2)).min(max_interval);
            }
        }
    };

    // Resolve the migrations directory in a way that works in release images too.
    // Preference order:
    //  1. config.migrations_dir (from config file or TC_DATABASE__MIGRATIONS_DIR env)
    //  2. ./migrations relative to the running binary
    //  3. The compile-time manifest directory for local `cargo run`
    let candidate_dirs = [
        config.migrations_dir.as_ref().map(PathBuf::from),
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
