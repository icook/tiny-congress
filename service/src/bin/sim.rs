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
    content::insert_sim_content,
    identity::SimAccount,
    llm::{generate_content, Usage},
    votes::cast_simulated_votes,
};

#[tokio::main]
#[allow(clippy::too_many_lines)]
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
        poll_duration_secs = config.poll_duration_secs,
        api_key_len = config.openrouter_api_key.len(),
        "sim config loaded"
    );

    // 3. Create HTTP client
    let http = reqwest::Client::new();
    let client = SimClient::new(http.clone(), config.api_url.clone());

    // 4. Set up verifier account (login to register device key)
    let verifier = SimAccount::verifier();
    tracing::info!(
        username = %verifier.username,
        root_pubkey = %verifier.root_pubkey_base64url(),
        "verifier identity (ensure TC_VERIFIERS includes this public key)"
    );

    let login_body = verifier.build_login_json();
    let resp = client.login(&login_body).await?;
    let login_status = resp.status();
    if login_status.as_u16() == 201 {
        tracing::info!("verifier device key registered via login");
    } else if login_status.as_u16() == 409 {
        tracing::debug!("verifier device key already registered");
    } else {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            status = %login_status,
            body = %body,
            "verifier login failed (account may not be bootstrapped yet)"
        );
    }

    // 5. Generate deterministic sim accounts
    let mut accounts: Vec<SimAccount> =
        (0..config.voter_count).map(SimAccount::from_seed).collect();
    tracing::info!(count = accounts.len(), "generated sim accounts");

    // 6. Sign up each account, endorse on 201
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

            // Endorse for voting eligibility via verifier device-key signing
            match client
                .endorse(&verifier, &account.username, "identity_verified")
                .await
            {
                Ok(()) => {
                    tracing::debug!(username = %account.username, "endorsed");
                }
                Err(e) => {
                    tracing::warn!(
                        username = %account.username,
                        error = %e,
                        "endorsement failed (verifier may not be bootstrapped)"
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

    // 7. Check total rooms and capacity via API; track token usage
    let mut session_usage = Usage::default();
    let all_rooms = client.list_rooms().await?;
    let total_rooms = all_rooms.len();
    tracing::info!(
        total_rooms,
        target = config.target_rooms,
        "room count check"
    );

    // Use account 0 as admin for content creation
    let admin = accounts.first().ok_or_else(|| {
        anyhow::anyhow!("voter_count must be >= 1 (need at least one account for content creation)")
    })?;

    // 8. If below target, create new rooms with content
    if total_rooms < config.target_rooms {
        let rooms_needed = config.target_rooms - total_rooms;
        tracing::info!(rooms_needed, "generating new rooms via LLM...");

        let (content, usage) = generate_content(&http, &config, rooms_needed).await?;
        session_usage += usage;
        tracing::info!(
            rooms_generated = content.rooms.len(),
            "LLM content received"
        );

        let result =
            insert_sim_content(&client, admin, &content, config.poll_duration_secs).await?;
        tracing::info!(
            rooms_created = result.rooms_created,
            rooms_skipped = result.rooms_skipped,
            polls_created = result.polls_created,
            "content inserted"
        );
    } else {
        tracing::info!("room target met, skipping room creation");
    }

    // 9. Check capacity — rooms needing new content (no active poll)
    let capacity_rooms = client.get_capacity().await?;
    if capacity_rooms.is_empty() {
        tracing::info!("all rooms have active polls, no capacity fill needed");
    } else {
        tracing::info!(
            rooms_needing_content = capacity_rooms.len(),
            "generating polls for rooms with capacity..."
        );

        let (content, usage) = generate_content(&http, &config, capacity_rooms.len()).await?;
        session_usage += usage;
        tracing::info!(
            rooms_generated = content.rooms.len(),
            "LLM content received for capacity fill"
        );

        // Create polls in existing rooms that need content
        let mut polls_created = 0;
        for (cap_room, sim_room) in capacity_rooms.iter().zip(content.rooms.iter()) {
            for poll in &sim_room.polls {
                let poll_resp = client
                    .create_poll(admin, cap_room.id, &poll.question, &poll.description)
                    .await?;

                for (i, dim) in poll.dimensions.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let sort_order = i as i32;
                    client
                        .add_dimension(
                            admin,
                            cap_room.id,
                            poll_resp.id,
                            &dim.name,
                            &dim.description,
                            dim.min,
                            dim.max,
                            sort_order,
                        )
                        .await?;
                }
                polls_created += 1;
            }
        }
        tracing::info!(polls_created, "capacity fill complete");
    }

    // 10. Cast simulated votes
    tracing::info!("casting simulated votes...");
    let vote_count = cast_simulated_votes(&client, &accounts, config.votes_per_poll).await?;
    tracing::info!(votes_cast = vote_count, "vote simulation complete");

    tracing::info!(
        prompt_tokens = session_usage.prompt_tokens,
        completion_tokens = session_usage.completion_tokens,
        total_tokens = session_usage.total_tokens,
        "llm_session_summary"
    );
    tracing::info!("tc-sim run complete");
    Ok(())
}
