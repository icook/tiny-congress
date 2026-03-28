// lint-patterns:allow-raw-pool — leaderboard and hall-of-fame need direct SQL joins
//! HTTP handlers for ranking-engine-specific operations.
//!
//! Covers submissions, matchups, leaderboard, hall of fame, and rounds.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{extract::Extension, extract::Query, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tc_engine_ranking::repo::rounds::{RoundRecord, RoundStatus};
use tc_engine_ranking::repo::submissions::{ContentType, SubmissionRecord};
use tc_engine_ranking::service::{RankingError, RankingService};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::http::{bad_request, conflict, forbidden, internal_error, not_found, Path};
use crate::identity::http::auth::AuthenticatedDevice;

// ─── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionResponse {
    pub id: String,
    pub round_id: String,
    pub content_type: String,
    pub url: Option<String>,
    pub image_key: Option<String>,
    pub caption: Option<String>,
    pub created_at: String,
    // author_id intentionally omitted — anonymous during ranking
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionWithAuthorResponse {
    pub id: String,
    pub round_id: String,
    pub author_id: String,
    pub content_type: String,
    pub url: Option<String>,
    pub image_key: Option<String>,
    pub caption: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MatchupResponse {
    pub submission_a: SubmissionResponse,
    pub submission_b: SubmissionResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MatchupResultResponse {
    pub id: String,
    pub winner_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardEntry {
    pub submission: SubmissionResponse,
    pub rating: f64,
    pub deviation: f64,
    pub matchup_count: i32,
    pub rank: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaderboardResponse {
    pub round_id: String,
    pub round_status: String,
    pub entries: Vec<LeaderboardEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RoundResponse {
    pub id: String,
    pub room_id: String,
    pub round_number: i32,
    pub submit_opens_at: String,
    pub rank_opens_at: String,
    pub closes_at: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HallOfFameEntryResponse {
    pub submission: SubmissionWithAuthorResponse,
    pub round_number: i32,
    pub final_rating: f64,
    pub rank: i32,
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitMemeRequest {
    pub content_type: String,
    pub url: Option<String>,
    pub image_key: Option<String>,
    pub caption: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordMatchupRequest {
    pub winner_id: Option<String>,
    pub loser_id: Option<String>,
    pub submission_a: Option<String>,
    pub submission_b: Option<String>,
    pub skipped: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

const fn default_limit() -> i64 {
    20
}

// ─── Converters ───────────────────────────────────────────────────────────────

const fn content_type_to_str(ct: &ContentType) -> &'static str {
    match ct {
        ContentType::Url => "url",
        ContentType::Image => "image",
    }
}

fn submission_to_response(s: &SubmissionRecord) -> SubmissionResponse {
    SubmissionResponse {
        id: s.id.to_string(),
        round_id: s.round_id.to_string(),
        content_type: content_type_to_str(&s.content_type).to_string(),
        url: s.url.clone(),
        image_key: s.image_key.clone(),
        caption: s.caption.clone(),
        created_at: s.created_at.to_rfc3339(),
    }
}

fn submission_to_response_with_author(s: &SubmissionRecord) -> SubmissionWithAuthorResponse {
    SubmissionWithAuthorResponse {
        id: s.id.to_string(),
        round_id: s.round_id.to_string(),
        author_id: s.author_id.to_string(),
        content_type: content_type_to_str(&s.content_type).to_string(),
        url: s.url.clone(),
        image_key: s.image_key.clone(),
        caption: s.caption.clone(),
        created_at: s.created_at.to_rfc3339(),
    }
}

const fn round_status_to_str(status: &RoundStatus) -> &'static str {
    match status {
        RoundStatus::Submitting => "submitting",
        RoundStatus::Ranking => "ranking",
        RoundStatus::Closed => "closed",
    }
}

fn round_to_response(r: &RoundRecord) -> RoundResponse {
    RoundResponse {
        id: r.id.to_string(),
        room_id: r.room_id.to_string(),
        round_number: r.round_number,
        submit_opens_at: r.submit_opens_at.to_rfc3339(),
        rank_opens_at: r.rank_opens_at.to_rfc3339(),
        closes_at: r.closes_at.to_rfc3339(),
        status: round_status_to_str(&r.status).to_string(),
    }
}

#[allow(clippy::result_large_err)]
fn parse_content_type(s: &str) -> Result<ContentType, axum::response::Response> {
    match s {
        "url" => Ok(ContentType::Url),
        "image" => Ok(ContentType::Image),
        other => Err(bad_request(&format!("unknown content_type: {other}"))),
    }
}

#[allow(clippy::result_large_err)]
fn parse_uuid(s: &str, field: &str) -> Result<Uuid, axum::response::Response> {
    s.parse::<Uuid>()
        .map_err(|_| bad_request(&format!("invalid UUID for {field}")))
}

fn ranking_error_response(e: RankingError) -> axum::response::Response {
    match e {
        RankingError::AlreadySubmitted => conflict("already submitted this round"),
        RankingError::NotInSubmitPhase | RankingError::NoActiveSubmitRound => {
            bad_request("not in submit phase")
        }
        RankingError::NotInRankingPhase | RankingError::NoActiveRankingRound => {
            bad_request("not in ranking phase")
        }
        RankingError::CannotRankOwn => forbidden("cannot rank your own submission"),
        RankingError::NoMatchupsAvailable => not_found("no matchups available"),
        RankingError::InvalidMatchup => {
            bad_request("invalid matchup: submissions not in this round")
        }
        RankingError::RoundNotFound => not_found("round not found"),
        RankingError::Internal(inner) => {
            tracing::error!("Ranking service internal error: {inner}");
            internal_error()
        }
    }
}

// ─── Submission handlers ──────────────────────────────────────────────────────

/// Submit a meme to the current round
#[utoipa::path(
    post,
    path = "/rooms/{room_id}/submissions",
    tag = "Ranking",
    params(("room_id" = String, Path, description = "Room ID")),
    request_body = SubmitMemeRequest,
    responses(
        (status = 201, description = "Submission created", body = SubmissionResponse),
        (status = 400, description = "Invalid request or not in submit phase"),
        (status = 409, description = "Already submitted this round"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn submit_meme(
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: SubmitMemeRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let content_type = match parse_content_type(&req.content_type) {
        Ok(ct) => ct,
        Err(resp) => return resp,
    };

    match ranking_service
        .submit(
            room_id,
            auth.account_id,
            content_type,
            req.url.as_deref(),
            req.image_key.as_deref(),
            req.caption.as_deref(),
        )
        .await
    {
        Ok(sub) => (StatusCode::CREATED, Json(submission_to_response(&sub))).into_response(),
        Err(e) => ranking_error_response(e),
    }
}

// ─── Matchup handlers ─────────────────────────────────────────────────────────

/// Get the next matchup pair to judge
#[utoipa::path(
    get,
    path = "/rooms/{room_id}/matchup",
    tag = "Ranking",
    params(("room_id" = String, Path, description = "Room ID")),
    responses(
        (status = 200, description = "Next matchup pair", body = MatchupResponse),
        (status = 404, description = "No matchups available"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_matchup(
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    match ranking_service
        .get_next_matchup(room_id, auth.account_id)
        .await
    {
        Ok(Some((sub_a, sub_b))) => {
            let response = MatchupResponse {
                submission_a: submission_to_response(&sub_a),
                submission_b: submission_to_response(&sub_b),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => not_found("no matchups available"),
        Err(e) => ranking_error_response(e),
    }
}

/// Record a matchup judgment (win/loss or skip)
#[utoipa::path(
    post,
    path = "/rooms/{room_id}/matchups",
    tag = "Ranking",
    params(("room_id" = String, Path, description = "Room ID")),
    request_body = RecordMatchupRequest,
    responses(
        (status = 201, description = "Matchup recorded", body = MatchupResultResponse),
        (status = 400, description = "Invalid request or not in ranking phase"),
        (status = 403, description = "Cannot rank own submission"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn record_matchup(
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
    Path(room_id): Path<Uuid>,
    auth: AuthenticatedDevice,
) -> impl IntoResponse {
    let req: RecordMatchupRequest = match auth.json() {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let result = if req.skipped == Some(true) {
        let Some(a_str) = req.submission_a.as_deref() else {
            return bad_request("submission_a required for skip");
        };
        let Some(b_str) = req.submission_b.as_deref() else {
            return bad_request("submission_b required for skip");
        };
        let sub_a = match parse_uuid(a_str, "submission_a") {
            Ok(id) => id,
            Err(resp) => return resp,
        };
        let sub_b = match parse_uuid(b_str, "submission_b") {
            Ok(id) => id,
            Err(resp) => return resp,
        };
        ranking_service
            .skip_matchup(room_id, auth.account_id, sub_a, sub_b)
            .await
    } else {
        let Some(winner_str) = req.winner_id.as_deref() else {
            return bad_request("winner_id required");
        };
        let Some(loser_str) = req.loser_id.as_deref() else {
            return bad_request("loser_id required");
        };
        let winner_id = match parse_uuid(winner_str, "winner_id") {
            Ok(id) => id,
            Err(resp) => return resp,
        };
        let loser_id = match parse_uuid(loser_str, "loser_id") {
            Ok(id) => id,
            Err(resp) => return resp,
        };
        ranking_service
            .record_matchup(room_id, auth.account_id, winner_id, loser_id)
            .await
    };

    match result {
        Ok(matchup) => {
            let response = MatchupResultResponse {
                id: matchup.id.to_string(),
                winner_id: matchup.winner_id.map(|id| id.to_string()),
                created_at: matchup.created_at.to_rfc3339(),
            };
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => ranking_error_response(e),
    }
}

// ─── Leaderboard handler ──────────────────────────────────────────────────────

/// Get the leaderboard for a specific round
#[utoipa::path(
    get,
    path = "/rooms/{room_id}/rounds/{round_id}/leaderboard",
    tag = "Ranking",
    params(
        ("room_id" = String, Path, description = "Room ID"),
        ("round_id" = String, Path, description = "Round ID"),
    ),
    responses(
        (status = 200, description = "Leaderboard for the round", body = LeaderboardResponse),
        (status = 404, description = "Round not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_leaderboard(
    Path((_room_id, round_id)): Path<(Uuid, Uuid)>,
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
    Extension(pool): Extension<PgPool>,
) -> impl IntoResponse {
    // Fetch the round to get status and room_id
    let round: Option<RoundRecord> = match sqlx::query_as::<_, RoundRecord>(
        r"
        SELECT id, room_id, round_number, submit_opens_at, rank_opens_at, closes_at,
               status, created_at
        FROM rooms__rounds
        WHERE id = $1
        ",
    )
    .bind(round_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to fetch round {round_id}: {e}");
            return internal_error();
        }
    };

    let Some(round) = round else {
        return not_found("round not found");
    };

    // Fetch rating records ordered by rating DESC
    let ratings = match ranking_service.get_leaderboard(round_id).await {
        Ok(r) => r,
        Err(e) => return ranking_error_response(e),
    };

    // Fetch all submissions for this round to join with ratings
    let submissions: Vec<SubmissionRecord> = match sqlx::query_as::<_, SubmissionRecord>(
        r"
        SELECT id, round_id, author_id, content_type, url, image_key, caption, created_at
        FROM rooms__submissions
        WHERE round_id = $1
        ORDER BY created_at ASC
        ",
    )
    .bind(round_id)
    .fetch_all(&pool)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to fetch submissions for round {round_id}: {e}");
            return internal_error();
        }
    };

    let sub_map: HashMap<Uuid, &SubmissionRecord> = submissions.iter().map(|s| (s.id, s)).collect();

    let entries: Vec<LeaderboardEntry> = ratings
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let sub = sub_map.get(&r.submission_id)?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let rank = (i + 1) as i32;
            Some(LeaderboardEntry {
                submission: submission_to_response(sub),
                rating: r.rating,
                deviation: r.deviation,
                matchup_count: r.matchup_count,
                rank,
            })
        })
        .collect();

    let response = LeaderboardResponse {
        round_id: round_id.to_string(),
        round_status: round_status_to_str(&round.status).to_string(),
        entries,
    };

    (StatusCode::OK, Json(response)).into_response()
}

// ─── Hall of fame handler ─────────────────────────────────────────────────────

/// Get hall of fame entries for a room
#[utoipa::path(
    get,
    path = "/rooms/{room_id}/hall-of-fame",
    tag = "Ranking",
    params(
        ("room_id" = String, Path, description = "Room ID"),
        ("limit" = Option<i64>, Query, description = "Max entries to return (default 20)"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination (default 0)"),
    ),
    responses(
        (status = 200, description = "Hall of fame entries", body = Vec<HallOfFameEntryResponse>),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_hall_of_fame(
    Path(room_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
    Extension(pool): Extension<PgPool>,
) -> impl IntoResponse {
    let hof_records = match ranking_service
        .get_hall_of_fame(room_id, params.limit, params.offset)
        .await
    {
        Ok(r) => r,
        Err(e) => return ranking_error_response(e),
    };

    if hof_records.is_empty() {
        return (StatusCode::OK, Json(Vec::<HallOfFameEntryResponse>::new())).into_response();
    }

    // Collect submission IDs and round IDs needed for enrichment
    let sub_ids: Vec<Uuid> = hof_records.iter().map(|h| h.submission_id).collect();
    let round_ids: Vec<Uuid> = hof_records.iter().map(|h| h.round_id).collect();

    // Fetch all relevant submissions in one query
    let submissions: Vec<SubmissionRecord> = match sqlx::query_as::<_, SubmissionRecord>(
        r"
        SELECT id, round_id, author_id, content_type, url, image_key, caption, created_at
        FROM rooms__submissions
        WHERE id = ANY($1)
        ",
    )
    .bind(&sub_ids)
    .fetch_all(&pool)
    .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to fetch submissions for hall of fame: {e}");
            return internal_error();
        }
    };

    let sub_map: HashMap<Uuid, &SubmissionRecord> = submissions.iter().map(|s| (s.id, s)).collect();

    // Fetch round numbers for display
    let rounds: Vec<RoundRecord> = match sqlx::query_as::<_, RoundRecord>(
        r"
        SELECT id, room_id, round_number, submit_opens_at, rank_opens_at, closes_at,
               status, created_at
        FROM rooms__rounds
        WHERE id = ANY($1)
        ",
    )
    .bind(&round_ids)
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to fetch rounds for hall of fame: {e}");
            return internal_error();
        }
    };

    let round_map: HashMap<Uuid, &RoundRecord> = rounds.iter().map(|r| (r.id, r)).collect();

    let entries: Vec<HallOfFameEntryResponse> = hof_records
        .iter()
        .filter_map(|h| {
            let sub = sub_map.get(&h.submission_id)?;
            let round = round_map.get(&h.round_id)?;
            Some(HallOfFameEntryResponse {
                submission: submission_to_response_with_author(sub),
                round_number: round.round_number,
                final_rating: h.final_rating,
                rank: h.rank,
            })
        })
        .collect();

    (StatusCode::OK, Json(entries)).into_response()
}

// ─── Round handlers ────────────────────────────────────────────────────────────

/// List all rounds for a room
#[utoipa::path(
    get,
    path = "/rooms/{room_id}/rounds",
    tag = "Ranking",
    params(("room_id" = String, Path, description = "Room ID")),
    responses(
        (status = 200, description = "All rounds for the room", body = Vec<RoundResponse>),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list_rounds(
    Path(room_id): Path<Uuid>,
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
) -> impl IntoResponse {
    match ranking_service.list_rounds(room_id).await {
        Ok(rounds) => {
            let resp: Vec<_> = rounds.iter().map(round_to_response).collect();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => ranking_error_response(e),
    }
}

/// Get current (non-closed) rounds for a room
#[utoipa::path(
    get,
    path = "/rooms/{room_id}/rounds/current",
    tag = "Ranking",
    params(("room_id" = String, Path, description = "Room ID")),
    responses(
        (status = 200, description = "Active rounds for the room", body = Vec<RoundResponse>),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_current_rounds(
    Path(room_id): Path<Uuid>,
    Extension(ranking_service): Extension<Arc<dyn RankingService>>,
) -> impl IntoResponse {
    match ranking_service.get_current_rounds(room_id).await {
        Ok(rounds) => {
            let resp: Vec<_> = rounds.iter().map(round_to_response).collect();
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(e) => ranking_error_response(e),
    }
}
