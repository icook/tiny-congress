//! Service layer for ranking operations.
//!
//! Orchestrates round lifecycle, submission validation, Glicko-2 rating updates,
//! pair selection, and hall-of-fame snapshotting. All repos are function-based
//! and called with `&self.pool`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::glicko2;
use crate::pair_selection::{select_pair, RatedSubmission};
use crate::repo::{
    hall_of_fame::{self, HallOfFameRecord},
    matchups::{self, MatchupRecord},
    ratings::{self, RatingRecord},
    rounds::{self, RoundRecord, RoundStatus},
    submissions::{self, ContentType, SubmissionRecord},
};

// ─── Error type ─────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum RankingError {
    #[error("round not found")]
    RoundNotFound,
    #[error("no active submit round")]
    NoActiveSubmitRound,
    #[error("no active ranking round")]
    NoActiveRankingRound,
    #[error("already submitted this round")]
    AlreadySubmitted,
    #[error("not in submit phase")]
    NotInSubmitPhase,
    #[error("not in ranking phase")]
    NotInRankingPhase,
    #[error("cannot rank own submission")]
    CannotRankOwn,
    #[error("no more matchups available")]
    NoMatchupsAvailable,
    #[error("invalid matchup: submissions not in this round")]
    InvalidMatchup,
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

// Convenience conversion from sqlx::Error
impl From<sqlx::Error> for RankingError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(anyhow::anyhow!(e))
    }
}

// ─── Service trait ──────────────────────────────────────────────────────────

#[async_trait]
pub trait RankingService: Send + Sync {
    async fn create_round(
        &self,
        room_id: Uuid,
        submit_opens_at: DateTime<Utc>,
        rank_opens_at: DateTime<Utc>,
        closes_at: DateTime<Utc>,
    ) -> Result<RoundRecord, RankingError>;

    async fn submit(
        &self,
        room_id: Uuid,
        author_id: Uuid,
        content_type: ContentType,
        url: Option<&str>,
        image_key: Option<&str>,
        caption: Option<&str>,
    ) -> Result<SubmissionRecord, RankingError>;

    async fn get_next_matchup(
        &self,
        room_id: Uuid,
        ranker_id: Uuid,
    ) -> Result<Option<(SubmissionRecord, SubmissionRecord)>, RankingError>;

    async fn record_matchup(
        &self,
        room_id: Uuid,
        ranker_id: Uuid,
        winner_id: Uuid,
        loser_id: Uuid,
    ) -> Result<MatchupRecord, RankingError>;

    async fn skip_matchup(
        &self,
        room_id: Uuid,
        ranker_id: Uuid,
        submission_a: Uuid,
        submission_b: Uuid,
    ) -> Result<MatchupRecord, RankingError>;

    async fn get_leaderboard(&self, round_id: Uuid) -> Result<Vec<RatingRecord>, RankingError>;

    async fn get_hall_of_fame(
        &self,
        room_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HallOfFameRecord>, RankingError>;

    async fn open_ranking(&self, round_id: Uuid) -> Result<(), RankingError>;

    async fn close_round(
        &self,
        round_id: Uuid,
        hall_of_fame_depth: i32,
    ) -> Result<(), RankingError>;

    async fn get_current_rounds(&self, room_id: Uuid) -> Result<Vec<RoundRecord>, RankingError>;

    async fn list_rounds(&self, room_id: Uuid) -> Result<Vec<RoundRecord>, RankingError>;
}

// ─── Implementation ─────────────────────────────────────────────────────────

pub struct DefaultRankingService {
    pool: PgPool,
}

impl DefaultRankingService {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Find the active submitting round for a room, if one exists.
async fn find_submitting_round(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Option<RoundRecord>, RankingError> {
    let rounds = rounds::get_current_rounds(pool, room_id).await?;
    Ok(rounds
        .into_iter()
        .find(|r| matches!(r.status, RoundStatus::Submitting)))
}

/// Find the active ranking round for a room, if one exists.
async fn find_ranking_round(
    pool: &PgPool,
    room_id: Uuid,
) -> Result<Option<RoundRecord>, RankingError> {
    let rounds = rounds::get_current_rounds(pool, room_id).await?;
    Ok(rounds
        .into_iter()
        .find(|r| matches!(r.status, RoundStatus::Ranking)))
}

#[async_trait]
impl RankingService for DefaultRankingService {
    async fn create_round(
        &self,
        room_id: Uuid,
        submit_opens_at: DateTime<Utc>,
        rank_opens_at: DateTime<Utc>,
        closes_at: DateTime<Utc>,
    ) -> Result<RoundRecord, RankingError> {
        let latest = rounds::get_latest_round_number(&self.pool, room_id).await?;
        let round_number = latest + 1;
        let record = rounds::create_round(
            &self.pool,
            room_id,
            round_number,
            submit_opens_at,
            rank_opens_at,
            closes_at,
        )
        .await?;
        Ok(record)
    }

    async fn submit(
        &self,
        room_id: Uuid,
        author_id: Uuid,
        content_type: ContentType,
        url: Option<&str>,
        image_key: Option<&str>,
        caption: Option<&str>,
    ) -> Result<SubmissionRecord, RankingError> {
        let round = find_submitting_round(&self.pool, room_id)
            .await?
            .ok_or(RankingError::NotInSubmitPhase)?;

        if submissions::has_submitted(&self.pool, round.id, author_id).await? {
            return Err(RankingError::AlreadySubmitted);
        }

        let record = submissions::create_submission(
            &self.pool,
            round.id,
            author_id,
            content_type,
            url,
            image_key,
            caption,
        )
        .await?;
        Ok(record)
    }

    async fn get_next_matchup(
        &self,
        room_id: Uuid,
        ranker_id: Uuid,
    ) -> Result<Option<(SubmissionRecord, SubmissionRecord)>, RankingError> {
        let Some(round) = find_ranking_round(&self.pool, room_id).await? else {
            return Ok(None);
        };

        let subs = submissions::list_submissions(&self.pool, round.id).await?;
        let rating_records = ratings::get_ratings_for_round(&self.pool, round.id).await?;

        // Build a lookup map from submission_id → rating record
        let rating_map: std::collections::HashMap<Uuid, &RatingRecord> = rating_records
            .iter()
            .map(|r| (r.submission_id, r))
            .collect();

        // Build RatedSubmission vec — skip any submission without a rating row
        let rated: Vec<RatedSubmission> = subs
            .iter()
            .filter_map(|s| {
                rating_map.get(&s.id).map(|r| RatedSubmission {
                    submission_id: s.id,
                    author_id: s.author_id,
                    rating: r.rating,
                    deviation: r.deviation,
                })
            })
            .collect();

        let judged = matchups::get_judged_pairs(&self.pool, round.id, ranker_id).await?;

        let pair = select_pair(&rated, &judged, ranker_id);
        let Some((a_id, b_id)) = pair else {
            return Ok(None);
        };

        // Look up the full submission records
        let sub_map: std::collections::HashMap<Uuid, SubmissionRecord> =
            subs.into_iter().map(|s| (s.id, s)).collect();

        let sub_a = sub_map
            .get(&a_id)
            .cloned()
            .ok_or_else(|| RankingError::Internal(anyhow::anyhow!("submission a missing")))?;
        let sub_b = sub_map
            .get(&b_id)
            .cloned()
            .ok_or_else(|| RankingError::Internal(anyhow::anyhow!("submission b missing")))?;

        Ok(Some((sub_a, sub_b)))
    }

    async fn record_matchup(
        &self,
        room_id: Uuid,
        ranker_id: Uuid,
        winner_id: Uuid,
        loser_id: Uuid,
    ) -> Result<MatchupRecord, RankingError> {
        let round = find_ranking_round(&self.pool, room_id)
            .await?
            .ok_or(RankingError::NotInRankingPhase)?;

        // Validate both submissions belong to this round
        let winner_sub = submissions::get_submission(&self.pool, winner_id)
            .await?
            .ok_or(RankingError::InvalidMatchup)?;
        let loser_sub = submissions::get_submission(&self.pool, loser_id)
            .await?
            .ok_or(RankingError::InvalidMatchup)?;

        if winner_sub.round_id != round.id || loser_sub.round_id != round.id {
            return Err(RankingError::InvalidMatchup);
        }

        // Validate ranker is not the author of either submission
        if winner_sub.author_id == ranker_id || loser_sub.author_id == ranker_id {
            return Err(RankingError::CannotRankOwn);
        }

        // Load current ratings (use defaults if missing)
        let winner_rating_rec = ratings::get_rating(&self.pool, winner_id).await?;
        let loser_rating_rec = ratings::get_rating(&self.pool, loser_id).await?;

        let mut winner_glicko = winner_rating_rec
            .as_ref()
            .map(|r| glicko2::Rating {
                rating: r.rating,
                deviation: r.deviation,
                volatility: r.volatility,
            })
            .unwrap_or_default();

        let mut loser_glicko = loser_rating_rec
            .as_ref()
            .map(|r| glicko2::Rating {
                rating: r.rating,
                deviation: r.deviation,
                volatility: r.volatility,
            })
            .unwrap_or_default();

        glicko2::update_ratings(&mut winner_glicko, &mut loser_glicko);

        let winner_matchup_count = winner_rating_rec.as_ref().map_or(0, |r| r.matchup_count) + 1;
        let loser_matchup_count = loser_rating_rec.as_ref().map_or(0, |r| r.matchup_count) + 1;

        // Persist atomically: create matchup + update both ratings
        let mut tx = self.pool.begin().await?;

        let matchup = matchups::create_matchup(
            &mut *tx,
            round.id,
            ranker_id,
            winner_id,
            loser_id,
            Some(winner_id),
        )
        .await?;

        ratings::update_rating(
            &mut *tx,
            winner_id,
            winner_glicko.rating,
            winner_glicko.deviation,
            winner_glicko.volatility,
            winner_matchup_count,
        )
        .await?;

        ratings::update_rating(
            &mut *tx,
            loser_id,
            loser_glicko.rating,
            loser_glicko.deviation,
            loser_glicko.volatility,
            loser_matchup_count,
        )
        .await?;

        tx.commit().await?;

        Ok(matchup)
    }

    async fn skip_matchup(
        &self,
        room_id: Uuid,
        ranker_id: Uuid,
        submission_a: Uuid,
        submission_b: Uuid,
    ) -> Result<MatchupRecord, RankingError> {
        let round = find_ranking_round(&self.pool, room_id)
            .await?
            .ok_or(RankingError::NotInRankingPhase)?;

        let matchup = matchups::create_matchup(
            &self.pool,
            round.id,
            ranker_id,
            submission_a,
            submission_b,
            None,
        )
        .await?;

        Ok(matchup)
    }

    async fn get_leaderboard(&self, round_id: Uuid) -> Result<Vec<RatingRecord>, RankingError> {
        let records = ratings::get_ratings_for_round(&self.pool, round_id).await?;
        Ok(records)
    }

    async fn get_hall_of_fame(
        &self,
        room_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HallOfFameRecord>, RankingError> {
        let records = hall_of_fame::list_hall_of_fame(&self.pool, room_id, limit, offset).await?;
        Ok(records)
    }

    async fn open_ranking(&self, round_id: Uuid) -> Result<(), RankingError> {
        let round = rounds::get_round(&self.pool, round_id)
            .await?
            .ok_or(RankingError::RoundNotFound)?;

        if !matches!(round.status, RoundStatus::Submitting) {
            return Err(RankingError::NotInSubmitPhase);
        }

        // Initialize ratings for all submissions in this round
        let subs = submissions::list_submissions(&self.pool, round_id).await?;
        let sub_ids: Vec<Uuid> = subs.iter().map(|s| s.id).collect();
        ratings::initialize_ratings(&self.pool, &sub_ids).await?;

        rounds::update_round_status(&self.pool, round_id, RoundStatus::Ranking).await?;

        Ok(())
    }

    async fn close_round(
        &self,
        round_id: Uuid,
        hall_of_fame_depth: i32,
    ) -> Result<(), RankingError> {
        let round = rounds::get_round(&self.pool, round_id)
            .await?
            .ok_or(RankingError::RoundNotFound)?;

        if !matches!(round.status, RoundStatus::Ranking) {
            return Err(RankingError::NotInRankingPhase);
        }

        // Get ratings ordered by rating DESC
        let rating_records = ratings::get_ratings_for_round(&self.pool, round_id).await?;

        // Take top N and build winners list: (submission_id, final_rating, rank).
        // hall_of_fame_depth is validated as non-negative by callers; a depth of 0 or
        // negative simply produces an empty list.
        let depth = usize::try_from(hall_of_fame_depth).unwrap_or(0);
        let winners: Vec<(Uuid, f64, i32)> = rating_records
            .iter()
            .take(depth)
            .enumerate()
            .map(|(i, r)| {
                // i is bounded by `depth` which fits in i32 for any realistic hall_of_fame_depth
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let rank = (i + 1) as i32;
                (r.submission_id, r.rating, rank)
            })
            .collect();

        hall_of_fame::insert_winners(&self.pool, round.room_id, round_id, &winners).await?;

        rounds::update_round_status(&self.pool, round_id, RoundStatus::Closed).await?;

        Ok(())
    }

    async fn get_current_rounds(&self, room_id: Uuid) -> Result<Vec<RoundRecord>, RankingError> {
        let records = rounds::get_current_rounds(&self.pool, room_id).await?;
        Ok(records)
    }

    async fn list_rounds(&self, room_id: Uuid) -> Result<Vec<RoundRecord>, RankingError> {
        let records = rounds::list_rounds(&self.pool, room_id).await?;
        Ok(records)
    }
}
