//! Content creation via the `TinyCongress` HTTP API.
//!
//! Orchestrates room, poll, and dimension creation through [`SimClient`]
//! instead of direct database access. Handles 409 conflicts on duplicate
//! room names by skipping the conflicting room.

use anyhow::Result;
use tracing::{debug, info};
use uuid::Uuid;

use super::client::SimClient;
use super::identity::SimAccount;
use super::llm::SimContent;

/// Summary of what [`insert_sim_content`] created (or skipped).
pub struct InsertResult {
    pub rooms_created: usize,
    pub rooms_skipped: usize,
    pub polls_created: usize,
}

/// Create rooms, polls, and dimensions via the HTTP API.
///
/// For each room in `content`:
/// 1. Create the room with `identity_verified` constraint and `verifier_id` in config.
/// 2. For each poll: create poll, add dimensions.
///    The rooms engine auto-activates the first poll.
///
/// `verifier_id` is the account UUID of the verifier that issues `identity_verified`
/// endorsements. If `None`, rooms are created without a verifier config (not recommended
/// for production — votes will be rejected until config is updated).
///
/// # Errors
///
/// Returns an error if any non-conflict API call fails.
pub async fn insert_sim_content(
    client: &SimClient,
    admin_account: &SimAccount,
    content: &SimContent,
    poll_duration_secs: i32,
    verifier_id: Option<Uuid>,
) -> Result<InsertResult, anyhow::Error> {
    let constraint_config = verifier_id.map(|id| serde_json::json!({"verifier_ids": [id]}));

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
                "identity_verified",
                constraint_config.as_ref(),
                Some(poll_duration_secs),
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
                        dim.min_label.as_deref(),
                        dim.max_label.as_deref(),
                    )
                    .await?;

                debug!(
                    poll_id = %poll_resp.id,
                    dimension = %dim.name,
                    sort_order = i,
                    "added dimension"
                );
            }

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
