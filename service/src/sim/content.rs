//! Content creation via the `TinyCongress` HTTP API.
//!
//! Orchestrates room, poll, and dimension creation through [`SimClient`]
//! instead of direct database access. Handles 409 conflicts on duplicate
//! room names by skipping the conflicting room.

use anyhow::Result;
use tracing::{debug, info};

use super::client::SimClient;
use super::identity::SimAccount;
use super::llm::SimContent;

/// Summary of what [`insert_sim_content`] created (or skipped).
pub struct InsertResult {
    pub rooms_created: usize,
    pub rooms_skipped: usize,
    pub polls_created: usize,
}

/// Count rooms that have at least one active poll.
///
/// Lists all rooms via the API, then for each open room checks whether
/// any of its polls are in `"active"` status.
///
/// # Errors
///
/// Returns an error if any API call fails.
pub async fn count_active_rooms(client: &SimClient) -> Result<usize, anyhow::Error> {
    let rooms = client.list_rooms().await?;
    let mut active = 0;
    for room in &rooms {
        if room.status != "open" {
            continue;
        }
        let polls = client.list_polls(room.id).await?;
        if polls.iter().any(|p| p.status == "active") {
            active += 1;
        }
    }
    Ok(active)
}

/// Create rooms, polls, and dimensions via the HTTP API.
///
/// For each room in `content`:
/// 1. Create the room (skip on 409 conflict).
/// 2. For each poll: create poll, add dimensions, activate.
///
/// # Errors
///
/// Returns an error if any non-conflict API call fails.
pub async fn insert_sim_content(
    client: &SimClient,
    admin_account: &SimAccount,
    content: &SimContent,
) -> Result<InsertResult, anyhow::Error> {
    let mut result = InsertResult {
        rooms_created: 0,
        rooms_skipped: 0,
        polls_created: 0,
    };

    for room in &content.rooms {
        let room_resp = match client
            .create_room(
                admin_account,
                &room.name,
                &room.description,
                "identity_verified",
            )
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                if e.to_string().contains("409") {
                    info!(room_name = %room.name, "room already exists, skipping");
                    result.rooms_skipped += 1;
                    continue;
                }
                return Err(e);
            }
        };

        result.rooms_created += 1;
        debug!(room_name = %room.name, room_id = %room_resp.id, "created room");

        for poll in &room.polls {
            let poll_resp = client
                .create_poll(
                    admin_account,
                    room_resp.id,
                    &poll.question,
                    &poll.description,
                )
                .await?;

            debug!(
                poll_id = %poll_resp.id,
                question = %poll.question,
                "created poll"
            );

            for (i, dim) in poll.dimensions.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let sort_order = i as i32;
                client
                    .add_dimension(
                        admin_account,
                        room_resp.id,
                        poll_resp.id,
                        &dim.name,
                        &dim.description,
                        dim.min,
                        dim.max,
                        sort_order,
                    )
                    .await?;

                debug!(
                    poll_id = %poll_resp.id,
                    dimension = %dim.name,
                    sort_order = i,
                    "added dimension"
                );
            }

            client
                .update_poll_status(admin_account, room_resp.id, poll_resp.id, "active")
                .await?;

            debug!(poll_id = %poll_resp.id, "activated poll");
            result.polls_created += 1;
        }
    }

    info!(
        rooms_created = result.rooms_created,
        rooms_skipped = result.rooms_skipped,
        polls_created = result.polls_created,
        "content insertion complete"
    );

    Ok(result)
}
