#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

use anyhow::Context;
use tinycongress_api::{config::Config, db::setup_database, seed::config::SeedConfig};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load database config from standard TC config path
    let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.logging.level).map_err(|e| {
                anyhow::anyhow!("invalid log level '{}': {e}", config.logging.level)
            })?,
        )
        .init();

    tracing::info!("tc-seed starting up");

    // Load seed-specific config from SEED_* env vars
    let seed_config = SeedConfig::from_env().context("failed to load seed config")?;
    tracing::info!(
        model = %seed_config.openrouter_model,
        target_rooms = seed_config.target_rooms,
        votes_per_poll = seed_config.votes_per_poll,
        voter_count = seed_config.voter_count,
        "seed config loaded"
    );

    // Connect to database (reuses existing retry logic and migrations)
    let pool = setup_database(&config.database).await?;

    tracing::info!("tc-seed run complete");
    pool.close().await;
    Ok(())
}
