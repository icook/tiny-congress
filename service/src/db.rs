use crate::config::DatabaseConfig;
use sqlx_core::migrate::Migrator;
use sqlx_postgres::{PgPool, PgPoolOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{info, warn};

/// Connect to the database and run migrations.
///
/// This function implements exponential backoff retry logic to handle
/// startup race conditions when the database container is still initializing.
///
/// # Errors
/// Returns an error if the database connection cannot be established or
/// migrations fail to run after exhausting retries.
pub async fn setup_database(config: &DatabaseConfig) -> Result<PgPool, anyhow::Error> {
    let retry_deadline = Duration::from_secs(60); // overall retry budget
    let max_interval = Duration::from_secs(30); // cap single waits
    let mut delay = Duration::from_millis(500);
    let start = Instant::now();

    let pool = loop {
        info!("Attempting to connect to Postgres...");

        match PgPoolOptions::new()
            .max_connections(config.max_connections)
            // Allow extra time to acquire a connection during startup bursts
            .acquire_timeout(Duration::from_secs(30))
            .connect_with(config.connect_options())
            .await
        {
            Ok(pool) => break pool,
            Err(err) => {
                if start.elapsed() >= retry_deadline {
                    warn!(error = %err, "Postgres not ready; retries exhausted");
                    return Err(err.into());
                }

                warn!(error = %err, "Postgres not ready yet; retrying");
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
