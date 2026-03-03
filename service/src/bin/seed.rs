#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

use anyhow::Context;
use tinycongress_api::{
    config::Config,
    db::setup_database,
    seed::{
        accounts::{ensure_endorsements, ensure_synthetic_accounts},
        config::SeedConfig,
        content::{count_active_rooms, insert_seed_content, list_active_polls_with_dimensions},
        llm::generate_content,
        votes::cast_simulated_votes,
    },
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::load().map_err(|e| anyhow::anyhow!("{e}"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.logging.level).map_err(|e| {
                anyhow::anyhow!("invalid log level '{}': {e}", config.logging.level)
            })?,
        )
        .init();

    tracing::info!("tc-seed starting up");

    let seed_config = SeedConfig::from_env().context("failed to load seed config")?;
    tracing::info!(
        model = %seed_config.openrouter_model,
        target_rooms = seed_config.target_rooms,
        votes_per_poll = seed_config.votes_per_poll,
        voter_count = seed_config.voter_count,
        "seed config loaded"
    );

    let pool = setup_database(&config.database).await?;

    // Step 1: Ensure synthetic accounts exist
    tracing::info!("ensuring synthetic accounts...");
    let accounts = ensure_synthetic_accounts(&pool, seed_config.voter_count).await?;
    tracing::info!(count = accounts.len(), "synthetic accounts ready");

    // Step 2: Ensure all accounts are endorsed for voting
    tracing::info!("ensuring endorsements...");
    ensure_endorsements(&pool, &accounts, "identity_verified").await?;
    tracing::info!("endorsements ready");

    // Step 3: Check how many active rooms exist
    let active_rooms = count_active_rooms(&pool).await?;
    tracing::info!(
        active_rooms,
        target = seed_config.target_rooms,
        "room count check"
    );

    if active_rooms < seed_config.target_rooms {
        let rooms_needed = seed_config.target_rooms - active_rooms;
        tracing::info!(rooms_needed, "generating new content via LLM...");

        // Step 4: Generate content via OpenRouter
        let client = reqwest::Client::new();
        let content = generate_content(&client, &seed_config, rooms_needed).await?;
        tracing::info!(
            rooms_generated = content.rooms.len(),
            "LLM content received"
        );

        // Step 5: Insert content into database
        let result = insert_seed_content(&pool, &content).await?;
        tracing::info!(
            rooms_created = result.rooms_created,
            rooms_skipped = result.rooms_skipped,
            polls_created = result.polls_created,
            "content inserted"
        );
    } else {
        tracing::info!("room target met, skipping content generation");
    }

    // Step 6: Cast simulated votes on all active polls
    tracing::info!("casting simulated votes...");
    let polls = list_active_polls_with_dimensions(&pool).await?;
    let vote_count =
        cast_simulated_votes(&pool, &accounts, &polls, seed_config.votes_per_poll).await?;
    tracing::info!(votes_cast = vote_count, "vote seeding complete");

    tracing::info!("tc-seed run complete");
    pool.close().await;
    Ok(())
}
