//! Integration tests for the ranking service layer.
//!
//! Tests run against a real `PostgreSQL` database via testcontainers.
//! Service tests use `isolated_db()` because the service operates on a full
//! `PgPool` (required by `DefaultRankingService`) rather than a raw transaction.

mod common;

use chrono::Utc;
use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_engine_ranking::repo::{rounds, submissions};
use tc_engine_ranking::service::{DefaultRankingService, RankingError, RankingService};
use tc_test_macros::shared_runtime_test;
use uuid::Uuid;

// ─── Fixtures ───────────────────────────────────────────────────────────────

/// Insert a minimal ranking room and return its ID.
async fn create_test_room(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO rooms__rooms (name, status, engine_type, engine_config) \
         VALUES ($1, 'open', 'ranking', '{}') RETURNING id",
    )
    .bind(format!("svc-test-room-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("create test room")
}

/// Build a timestamp offset by `secs` seconds from now.
fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc::now() + chrono::Duration::seconds(secs)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_submit_succeeds_during_submit_phase() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let author = AccountFactory::new()
        .create(&pool)
        .await
        .expect("create author");

    let service = DefaultRankingService::new(pool);

    // Create a round in submitting status
    service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    let result = service
        .submit(
            room_id,
            author.id,
            submissions::ContentType::Url,
            Some("https://example.com/meme.png"),
            None,
            Some("A great meme"),
        )
        .await
        .expect("submit");

    assert_eq!(result.author_id, author.id);
    assert_eq!(result.url.as_deref(), Some("https://example.com/meme.png"));
}

#[shared_runtime_test]
async fn test_submit_rejects_duplicate() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let author = AccountFactory::new()
        .create(&pool)
        .await
        .expect("create author");

    let service = DefaultRankingService::new(pool);

    service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    service
        .submit(
            room_id,
            author.id,
            submissions::ContentType::Url,
            Some("https://example.com/first.png"),
            None,
            None,
        )
        .await
        .expect("first submit");

    let second = service
        .submit(
            room_id,
            author.id,
            submissions::ContentType::Url,
            Some("https://example.com/second.png"),
            None,
            None,
        )
        .await;

    assert!(
        matches!(second, Err(RankingError::AlreadySubmitted)),
        "expected AlreadySubmitted, got: {second:?}"
    );
}

#[shared_runtime_test]
async fn test_submit_rejects_when_not_submitting() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let author = AccountFactory::new()
        .create(&pool)
        .await
        .expect("create author");

    let service = DefaultRankingService::new(pool.clone());

    // Create a round and advance it to ranking status
    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    // Manually move to ranking status (open_ranking requires submissions; skip that here)
    rounds::update_round_status(&pool, round.id, rounds::RoundStatus::Ranking)
        .await
        .expect("update to ranking");

    let result = service
        .submit(
            room_id,
            author.id,
            submissions::ContentType::Url,
            Some("https://example.com/meme.png"),
            None,
            None,
        )
        .await;

    assert!(
        matches!(result, Err(RankingError::NotInSubmitPhase)),
        "expected NotInSubmitPhase, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_open_ranking_initializes_ratings() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    // Submit two entries
    service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    service.open_ranking(round.id).await.expect("open ranking");

    // Verify ratings exist for all submissions
    let ratings = tc_engine_ranking::repo::ratings::get_ratings_for_round(&pool, round.id)
        .await
        .expect("get ratings");

    assert_eq!(ratings.len(), 2, "both submissions should have ratings");
    for r in &ratings {
        assert!(
            (r.rating - 1500.0).abs() < f64::EPSILON,
            "default rating should be 1500"
        );
    }
}

#[shared_runtime_test]
async fn test_get_next_matchup_returns_pair() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");
    let ranker = AccountFactory::new().create(&pool).await.expect("ranker");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    service.open_ranking(round.id).await.expect("open ranking");

    let matchup = service
        .get_next_matchup(room_id, ranker.id)
        .await
        .expect("get next matchup");

    assert!(matchup.is_some(), "should return a pair");
    let (sub_a, sub_b) = matchup.unwrap();
    assert_ne!(
        sub_a.id, sub_b.id,
        "pair should be two different submissions"
    );
}

#[shared_runtime_test]
async fn test_get_next_matchup_returns_none_when_exhausted() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");
    let ranker = AccountFactory::new().create(&pool).await.expect("ranker");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    service.open_ranking(round.id).await.expect("open ranking");

    // Get the pair and record a matchup to exhaust all pairs
    let (sub_a, sub_b) = service
        .get_next_matchup(room_id, ranker.id)
        .await
        .expect("get first matchup")
        .expect("first pair exists");

    service
        .record_matchup(room_id, ranker.id, sub_a.id, sub_b.id)
        .await
        .expect("record matchup");

    // Now the only pair has been judged — should return None
    let second = service
        .get_next_matchup(room_id, ranker.id)
        .await
        .expect("get next matchup after exhaustion");

    assert!(
        second.is_none(),
        "expected None after all pairs exhausted, got: {second:?}"
    );
}

#[shared_runtime_test]
async fn test_record_matchup_updates_ratings() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");
    let ranker = AccountFactory::new().create(&pool).await.expect("ranker");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    let sub1 = service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    let sub2 = service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    service.open_ranking(round.id).await.expect("open ranking");

    // sub1 wins
    service
        .record_matchup(room_id, ranker.id, sub1.id, sub2.id)
        .await
        .expect("record matchup");

    let ratings = tc_engine_ranking::repo::ratings::get_ratings_for_round(&pool, round.id)
        .await
        .expect("get ratings");

    let winner_rating = ratings
        .iter()
        .find(|r| r.submission_id == sub1.id)
        .expect("winner rating");
    let loser_rating = ratings
        .iter()
        .find(|r| r.submission_id == sub2.id)
        .expect("loser rating");

    assert!(
        winner_rating.rating > 1500.0,
        "winner rating should increase above 1500, got {}",
        winner_rating.rating
    );
    assert!(
        loser_rating.rating < 1500.0,
        "loser rating should decrease below 1500, got {}",
        loser_rating.rating
    );
    assert_eq!(winner_rating.matchup_count, 1);
    assert_eq!(loser_rating.matchup_count, 1);
}

#[shared_runtime_test]
async fn test_skip_matchup_no_rating_change() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");
    let ranker = AccountFactory::new().create(&pool).await.expect("ranker");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    let sub1 = service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    let sub2 = service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    service.open_ranking(round.id).await.expect("open ranking");

    service
        .skip_matchup(room_id, ranker.id, sub1.id, sub2.id)
        .await
        .expect("skip matchup");

    let ratings = tc_engine_ranking::repo::ratings::get_ratings_for_round(&pool, round.id)
        .await
        .expect("get ratings");

    // Ratings should be unchanged (still default 1500)
    for r in &ratings {
        assert!(
            (r.rating - 1500.0).abs() < f64::EPSILON,
            "rating should remain 1500 after skip, got {}",
            r.rating
        );
    }
}

#[shared_runtime_test]
async fn test_close_round_snapshots_hall_of_fame() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");
    let a3 = AccountFactory::new().create(&pool).await.expect("a3");
    let ranker = AccountFactory::new().create(&pool).await.expect("ranker");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    let sub1 = service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    let sub2 = service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    service
        .submit(
            room_id,
            a3.id,
            submissions::ContentType::Url,
            Some("https://example.com/3.png"),
            None,
            None,
        )
        .await
        .expect("submit 3");

    service.open_ranking(round.id).await.expect("open ranking");

    // sub1 wins a matchup to have a higher rating
    service
        .record_matchup(room_id, ranker.id, sub1.id, sub2.id)
        .await
        .expect("record matchup");

    // Close round with top 2
    service.close_round(round.id, 2).await.expect("close round");

    let hof = service
        .get_hall_of_fame(room_id, 10, 0)
        .await
        .expect("get hall of fame");

    assert_eq!(hof.len(), 2, "should have exactly 2 hall of fame entries");

    // The round should now be closed
    let updated_round = rounds::get_round(&pool, round.id)
        .await
        .expect("get round")
        .expect("round exists");
    assert!(
        matches!(updated_round.status, rounds::RoundStatus::Closed),
        "round should be closed"
    );
}

#[shared_runtime_test]
async fn test_leaderboard_ordered_by_rating() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let room_id = create_test_room(&pool).await;
    let a1 = AccountFactory::new().create(&pool).await.expect("a1");
    let a2 = AccountFactory::new().create(&pool).await.expect("a2");
    let a3 = AccountFactory::new().create(&pool).await.expect("a3");
    let ranker = AccountFactory::new().create(&pool).await.expect("ranker");

    let service = DefaultRankingService::new(pool.clone());

    let round = service
        .create_round(room_id, ts(0), ts(3600), ts(7200))
        .await
        .expect("create round");

    let sub1 = service
        .submit(
            room_id,
            a1.id,
            submissions::ContentType::Url,
            Some("https://example.com/1.png"),
            None,
            None,
        )
        .await
        .expect("submit 1");

    let sub2 = service
        .submit(
            room_id,
            a2.id,
            submissions::ContentType::Url,
            Some("https://example.com/2.png"),
            None,
            None,
        )
        .await
        .expect("submit 2");

    let sub3 = service
        .submit(
            room_id,
            a3.id,
            submissions::ContentType::Url,
            Some("https://example.com/3.png"),
            None,
            None,
        )
        .await
        .expect("submit 3");

    service.open_ranking(round.id).await.expect("open ranking");

    // sub1 beats sub2 and sub3
    service
        .record_matchup(room_id, ranker.id, sub1.id, sub2.id)
        .await
        .expect("matchup 1 vs 2");

    service
        .record_matchup(room_id, ranker.id, sub1.id, sub3.id)
        .await
        .expect("matchup 1 vs 3");

    let leaderboard = service
        .get_leaderboard(round.id)
        .await
        .expect("get leaderboard");

    assert_eq!(leaderboard.len(), 3);

    // Verify descending order
    for i in 0..(leaderboard.len() - 1) {
        assert!(
            leaderboard[i].rating >= leaderboard[i + 1].rating,
            "leaderboard should be ordered DESC by rating, but [{i}]={} and [{}]={}",
            leaderboard[i].rating,
            i + 1,
            leaderboard[i + 1].rating
        );
    }

    // sub1 won twice and should be at the top
    assert_eq!(
        leaderboard[0].submission_id, sub1.id,
        "sub1 (2 wins) should be first"
    );
}
