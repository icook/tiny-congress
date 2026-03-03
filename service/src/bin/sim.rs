#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]

use anyhow::Context;
use tinycongress_api::sim::{
    client::SimClient,
    config::SimConfig,
    content::{count_active_rooms, insert_sim_content},
    identity::SimAccount,
    llm::generate_content,
    votes::cast_simulated_votes,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. Load sim config from SIM_* env vars
    let config = SimConfig::from_env().context("failed to load sim config")?;

    // 2. Init tracing with configured log level
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&config.log_level)
                .map_err(|e| anyhow::anyhow!("invalid log level '{}': {e}", config.log_level))?,
        )
        .init();

    tracing::info!("tc-sim starting up");
    tracing::info!(
        api_url = %config.api_url,
        model = %config.openrouter_model,
        target_rooms = config.target_rooms,
        votes_per_poll = config.votes_per_poll,
        voter_count = config.voter_count,
        "sim config loaded"
    );

    // 3. Create HTTP client
    let http = reqwest::Client::new();
    let client = SimClient::new(http.clone(), config.api_url.clone());

    // 4. Generate deterministic sim accounts
    let mut accounts: Vec<SimAccount> =
        (0..config.voter_count).map(SimAccount::from_seed).collect();
    tracing::info!(count = accounts.len(), "generated sim accounts");

    // 5. Sign up each account, endorse on 201
    tracing::info!("signing up accounts...");
    for account in &mut accounts {
        let signup_body = account
            .build_signup_json()
            .context("failed to build signup JSON")?;
        let resp = client.signup(&signup_body).await?;
        let status = resp.status();

        if status.as_u16() == 201 {
            // New account — parse response to get account_id, then endorse
            let signup_resp: tinycongress_api::sim::client::SignupResponse = resp
                .json()
                .await
                .context("failed to parse signup response")?;
            account.account_id = Some(signup_resp.account_id);
            tracing::info!(username = %account.username, "created account");

            // Endorse for voting eligibility
            match client
                .endorse(
                    &config.verifier_api_key,
                    &account.username,
                    "identity_verified",
                )
                .await
            {
                Ok(()) => {
                    tracing::debug!(username = %account.username, "endorsed");
                }
                Err(e) => {
                    tracing::warn!(
                        username = %account.username,
                        error = %e,
                        "endorsement failed (verifier API may not be available)"
                    );
                }
            }
        } else if status.as_u16() == 409 {
            // Already exists from a previous run — keys are deterministic
            tracing::debug!(username = %account.username, "account already exists");
        } else {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "signup for {} returned {status}: {body}",
                account.username
            ));
        }
    }
    tracing::info!("account signup complete");

    // 6. Count active rooms via API
    let active_rooms = count_active_rooms(&client).await?;
    tracing::info!(
        active_rooms,
        target = config.target_rooms,
        "room count check"
    );

    // 7. If below target, generate and insert content
    if active_rooms < config.target_rooms {
        let rooms_needed = config.target_rooms - active_rooms;
        tracing::info!(rooms_needed, "generating new content via LLM...");

        let content = generate_content(&http, &config, rooms_needed).await?;
        tracing::info!(
            rooms_generated = content.rooms.len(),
            "LLM content received"
        );

        // Use account 0 as admin for content creation
        let admin = accounts.first().ok_or_else(|| {
            anyhow::anyhow!(
                "voter_count must be >= 1 (need at least one account for content creation)"
            )
        })?;
        let result = insert_sim_content(&client, admin, &content).await?;
        tracing::info!(
            rooms_created = result.rooms_created,
            rooms_skipped = result.rooms_skipped,
            polls_created = result.polls_created,
            "content inserted"
        );
    } else {
        tracing::info!("room target met, skipping content generation");
    }

    // 8. Cast simulated votes
    tracing::info!("casting simulated votes...");
    let vote_count = cast_simulated_votes(&client, &accounts, config.votes_per_poll).await?;
    tracing::info!(votes_cast = vote_count, "vote simulation complete");

    tracing::info!("tc-sim run complete");
    Ok(())
}
