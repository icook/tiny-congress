//! Vote simulation for demo seeding.
//!
//! Casts reproducible simulated votes from synthetic accounts using a
//! seeded RNG so that re-running the seed worker produces identical results.

use rand::prelude::*;
use rand::rngs::StdRng;
use sqlx::PgPool;
use uuid::Uuid;

use super::accounts::SyntheticAccount;
use crate::rooms::repo::votes::{count_voters, upsert_vote};

/// A poll and its dimensions: `(poll_id, Vec<(dimension_id, min_value, max_value)>)`.
type PollWithDimensions = (Uuid, Vec<(Uuid, f32, f32)>);

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
/// Uses a deterministic seed (`20260303`) so votes are reproducible across
/// runs. For each poll, the function checks how many voters have already
/// cast ballots and only inserts the remaining votes needed to reach
/// `votes_per_poll`. The `upsert_vote` repo function handles ON CONFLICT,
/// making re-runs safe.
///
/// Returns the total number of individual vote records inserted.
///
/// # Errors
///
/// Returns an error if a database operation fails.
pub async fn cast_simulated_votes(
    pool: &PgPool,
    accounts: &[SyntheticAccount],
    polls: &[PollWithDimensions],
    votes_per_poll: usize,
) -> Result<usize, anyhow::Error> {
    let mut rng = StdRng::seed_from_u64(20_260_303);
    let voter_count = votes_per_poll.min(accounts.len());
    let mut total_votes: usize = 0;

    for (poll_id, dimensions) in polls {
        let existing = count_voters(pool, *poll_id).await?;

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let existing_usize = existing.max(0) as usize;

        if existing_usize >= voter_count {
            tracing::debug!(poll_id = %poll_id, existing = existing_usize, "poll already has enough voters, skipping");
            continue;
        }

        for account in &accounts[existing_usize..voter_count] {
            for (dimension_id, min_val, max_val) in dimensions {
                let value = random_vote_value(&mut rng, *min_val, *max_val);
                upsert_vote(pool, *poll_id, *dimension_id, account.id, value).await?;
                total_votes += 1;
            }
        }

        tracing::info!(
            poll_id = %poll_id,
            new_voters = voter_count - existing_usize,
            "cast simulated votes"
        );
    }

    Ok(total_votes)
}

#[cfg(test)]
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
