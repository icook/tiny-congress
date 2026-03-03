//! Vote simulation via the `TinyCongress` HTTP API.
//!
//! Discovers active polls through API calls and casts reproducible simulated
//! votes from synthetic accounts using a seeded RNG. Re-running with the same
//! accounts and content produces identical vote patterns (the API's upsert
//! semantics make this idempotent).

use rand::prelude::*;
use rand::rngs::StdRng;

use super::client::SimClient;
use super::identity::SimAccount;

/// Generate a random vote value within `[min, max]` using a triangular-ish
/// distribution (average of two uniform samples) for more realistic spread
/// than pure uniform.
pub fn random_vote_value(rng: &mut StdRng, min: f32, max: f32) -> f32 {
    let u1: f32 = rng.gen();
    let u2: f32 = rng.gen();
    let t = (u1 + u2) / 2.0;
    min + t * (max - min)
}

/// Cast simulated votes from synthetic accounts across all active polls.
///
/// Uses a deterministic seed (`20_260_303`) so votes are reproducible across
/// runs. Discovers polls via the API, checks existing voter counts, and only
/// casts the remaining votes needed to reach `votes_per_poll`.
///
/// Returns the total number of successful vote casts (one per account per poll).
///
/// # Errors
///
/// Returns an error if a non-recoverable API call fails. A 403 on `cast_vote`
/// (user not eligible) is logged as a warning and skipped.
pub async fn cast_simulated_votes(
    client: &SimClient,
    accounts: &[SimAccount],
    votes_per_poll: usize,
) -> Result<usize, anyhow::Error> {
    let mut rng = StdRng::seed_from_u64(20_260_303);
    let voter_count = votes_per_poll.min(accounts.len());
    let mut total_votes: usize = 0;

    // Step 1: discover open rooms
    let rooms = client.list_rooms().await?;
    let open_rooms: Vec<_> = rooms.into_iter().filter(|r| r.status == "open").collect();

    for room in &open_rooms {
        // Step 2: discover active polls in this room
        let polls = client.list_polls(room.id).await?;
        let active_polls: Vec<_> = polls.into_iter().filter(|p| p.status == "active").collect();

        for poll in &active_polls {
            // Step 3: get dimensions for this poll
            let detail = client.get_poll_detail(room.id, poll.id).await?;

            // Step 4: check existing voter count
            let results = client.get_poll_results(room.id, poll.id).await?;

            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let existing_usize = results.voter_count.max(0) as usize;

            if existing_usize >= voter_count {
                tracing::debug!(
                    poll_id = %poll.id,
                    existing = existing_usize,
                    "poll already has enough voters, skipping"
                );
                continue;
            }

            // Step 5: cast votes for remaining accounts
            for account in &accounts[existing_usize..voter_count] {
                let votes: Vec<_> = detail
                    .dimensions
                    .iter()
                    .map(|dim| {
                        let value = random_vote_value(&mut rng, dim.min_value, dim.max_value);
                        (dim.id, value)
                    })
                    .collect();

                match client.cast_vote(account, room.id, poll.id, &votes).await {
                    Ok(_) => {
                        total_votes += 1;
                    }
                    Err(e) if e.to_string().contains("403") => {
                        tracing::warn!(
                            username = %account.username,
                            poll_id = %poll.id,
                            "not eligible to vote (403), skipping"
                        );
                    }
                    Err(e) => return Err(e),
                }
            }

            tracing::info!(
                poll_id = %poll.id,
                new_voters = voter_count - existing_usize,
                "cast simulated votes"
            );
        }
    }

    Ok(total_votes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn generates_values_within_bounds() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let value = random_vote_value(&mut rng, 0.0, 10.0);
            assert!((0.0..=10.0).contains(&value), "value {value} out of range");
        }
    }

    #[test]
    fn different_seeds_produce_different_values() {
        let mut rng1 = StdRng::seed_from_u64(1);
        let mut rng2 = StdRng::seed_from_u64(2);
        let v1 = random_vote_value(&mut rng1, 0.0, 10.0);
        let v2 = random_vote_value(&mut rng2, 0.0, 10.0);
        assert!((v1 - v2).abs() > f32::EPSILON);
    }

    #[test]
    fn same_seed_is_reproducible() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let v1 = random_vote_value(&mut rng1, 0.0, 10.0);
        let v2 = random_vote_value(&mut rng2, 0.0, 10.0);
        assert!((v1 - v2).abs() < f32::EPSILON);
    }
}
