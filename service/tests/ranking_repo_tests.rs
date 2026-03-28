//! Integration tests for the ranking engine repo layer.
//!
//! Tests run against a real PostgreSQL database via testcontainers.
//! Each test uses a transaction that rolls back on completion, so tests
//! are independent.

mod common;

use chrono::Utc;
use common::factories::AccountFactory;
use common::test_db::test_transaction;
use tc_engine_ranking::repo::{hall_of_fame, matchups, ratings, rounds, submissions};
use tc_test_macros::shared_runtime_test;
use uuid::Uuid;

// ─── Fixtures ───────────────────────────────────────────────────────────────

/// Insert a minimal room and return its ID.
async fn create_test_room(tx: &mut sqlx::PgConnection) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO rooms__rooms (name, status, engine_type, engine_config) \
         VALUES ($1, 'open', 'ranking', '{}') RETURNING id",
    )
    .bind(format!("test-room-{}", Uuid::new_v4()))
    .fetch_one(tx)
    .await
    .expect("create test room")
}

/// Build a timestamp offset by `secs` seconds from now.
fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc::now() + chrono::Duration::seconds(secs)
}

// ─── Round tests ────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_create_round_returns_all_fields() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let submit_opens = ts(0);
    let rank_opens = ts(3600);
    let closes = ts(7200);

    let record = rounds::create_round(&mut *tx, room_id, 1, submit_opens, rank_opens, closes)
        .await
        .expect("create round");

    assert_eq!(record.room_id, room_id);
    assert_eq!(record.round_number, 1);
    assert!(matches!(record.status, rounds::RoundStatus::Submitting));
}

#[shared_runtime_test]
async fn test_round_status_transitions() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = rounds::create_round(&mut *tx, room_id, 1, ts(0), ts(1), ts(2))
        .await
        .expect("create round");

    rounds::update_round_status(&mut *tx, round.id, rounds::RoundStatus::Ranking)
        .await
        .expect("to ranking");

    let r = rounds::get_round(&mut *tx, round.id)
        .await
        .expect("get round")
        .expect("round exists");
    assert!(matches!(r.status, rounds::RoundStatus::Ranking));

    rounds::update_round_status(&mut *tx, round.id, rounds::RoundStatus::Closed)
        .await
        .expect("to closed");

    let r = rounds::get_round(&mut *tx, round.id)
        .await
        .expect("get round")
        .expect("round exists");
    assert!(matches!(r.status, rounds::RoundStatus::Closed));
}

#[shared_runtime_test]
async fn test_get_latest_round_number_empty_then_correct() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;

    let n = rounds::get_latest_round_number(&mut *tx, room_id)
        .await
        .expect("latest round number (empty)");
    assert_eq!(n, 0, "no rounds → should return 0");

    rounds::create_round(&mut *tx, room_id, 1, ts(0), ts(1), ts(2))
        .await
        .expect("create round 1");
    rounds::create_round(&mut *tx, room_id, 5, ts(0), ts(1), ts(2))
        .await
        .expect("create round 5");

    let n = rounds::get_latest_round_number(&mut *tx, room_id)
        .await
        .expect("latest round number");
    assert_eq!(n, 5);
}

#[shared_runtime_test]
async fn test_get_current_rounds_excludes_closed() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;

    let r1 = rounds::create_round(&mut *tx, room_id, 1, ts(0), ts(1), ts(2))
        .await
        .expect("round 1");
    rounds::create_round(&mut *tx, room_id, 2, ts(0), ts(1), ts(2))
        .await
        .expect("round 2");

    // Close round 1.
    rounds::update_round_status(&mut *tx, r1.id, rounds::RoundStatus::Closed)
        .await
        .expect("close round 1");

    let current = rounds::get_current_rounds(&mut *tx, room_id)
        .await
        .expect("get current rounds");

    assert_eq!(current.len(), 1);
    assert_eq!(current[0].round_number, 2);
}

#[shared_runtime_test]
async fn test_list_rounds_ordered_desc() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;

    rounds::create_round(&mut *tx, room_id, 1, ts(0), ts(1), ts(2))
        .await
        .expect("round 1");
    rounds::create_round(&mut *tx, room_id, 2, ts(0), ts(1), ts(2))
        .await
        .expect("round 2");
    rounds::create_round(&mut *tx, room_id, 3, ts(0), ts(1), ts(2))
        .await
        .expect("round 3");

    let all = rounds::list_rounds(&mut *tx, room_id)
        .await
        .expect("list rounds");

    assert_eq!(all.len(), 3);
    assert_eq!(all[0].round_number, 3, "DESC order: first should be 3");
    assert_eq!(all[2].round_number, 1, "DESC order: last should be 1");
}

// ─── Submission tests ────────────────────────────────────────────────────────

async fn create_test_round(
    tx: &mut sqlx::PgConnection,
    room_id: Uuid,
    round_number: i32,
) -> rounds::RoundRecord {
    rounds::create_round(tx, room_id, round_number, ts(0), ts(1), ts(2))
        .await
        .expect("create test round")
}

#[shared_runtime_test]
async fn test_create_submission_url_type() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let author = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("account");

    let record = submissions::create_submission(
        &mut *tx,
        round.id,
        author.id,
        submissions::ContentType::Url,
        Some("https://example.com/meme.png"),
        None,
        Some("A great meme"),
    )
    .await
    .expect("create url submission");

    assert_eq!(record.round_id, round.id);
    assert_eq!(record.author_id, author.id);
    assert!(matches!(record.content_type, submissions::ContentType::Url));
    assert_eq!(record.url.as_deref(), Some("https://example.com/meme.png"));
    assert!(record.image_key.is_none());
    assert_eq!(record.caption.as_deref(), Some("A great meme"));
}

#[shared_runtime_test]
async fn test_create_submission_image_type() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let author = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("account");

    let record = submissions::create_submission(
        &mut *tx,
        round.id,
        author.id,
        submissions::ContentType::Image,
        None,
        Some("uploads/abc123.png"),
        None,
    )
    .await
    .expect("create image submission");

    assert!(matches!(
        record.content_type,
        submissions::ContentType::Image
    ));
    assert!(record.url.is_none());
    assert_eq!(record.image_key.as_deref(), Some("uploads/abc123.png"));
}

#[shared_runtime_test]
async fn test_duplicate_submission_fails() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let author = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("account");

    submissions::create_submission(
        &mut *tx,
        round.id,
        author.id,
        submissions::ContentType::Url,
        Some("https://example.com/a.png"),
        None,
        None,
    )
    .await
    .expect("first submission");

    let result = submissions::create_submission(
        &mut *tx,
        round.id,
        author.id,
        submissions::ContentType::Url,
        Some("https://example.com/b.png"),
        None,
        None,
    )
    .await;

    assert!(result.is_err(), "duplicate (round, author) should fail");
}

#[shared_runtime_test]
async fn test_has_submitted_returns_correct_bool() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let author = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("account");
    let other = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("other account");

    let before = submissions::has_submitted(&mut *tx, round.id, author.id)
        .await
        .expect("has_submitted before");
    assert!(!before);

    submissions::create_submission(
        &mut *tx,
        round.id,
        author.id,
        submissions::ContentType::Url,
        Some("https://example.com/a.png"),
        None,
        None,
    )
    .await
    .expect("create submission");

    let after = submissions::has_submitted(&mut *tx, round.id, author.id)
        .await
        .expect("has_submitted after");
    assert!(after);

    let other_check = submissions::has_submitted(&mut *tx, round.id, other.id)
        .await
        .expect("has_submitted other");
    assert!(!other_check);
}

#[shared_runtime_test]
async fn test_count_submissions() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;

    let a1 = AccountFactory::new().create(&mut *tx).await.expect("a1");
    let a2 = AccountFactory::new().create(&mut *tx).await.expect("a2");
    let a3 = AccountFactory::new().create(&mut *tx).await.expect("a3");

    let count0 = submissions::count_submissions(&mut *tx, round.id)
        .await
        .expect("count 0");
    assert_eq!(count0, 0);

    for (account, url) in [
        (&a1, "https://example.com/1.png"),
        (&a2, "https://example.com/2.png"),
        (&a3, "https://example.com/3.png"),
    ] {
        submissions::create_submission(
            &mut *tx,
            round.id,
            account.id,
            submissions::ContentType::Url,
            Some(url),
            None,
            None,
        )
        .await
        .expect("create submission");
    }

    let count3 = submissions::count_submissions(&mut *tx, round.id)
        .await
        .expect("count 3");
    assert_eq!(count3, 3);
}

// ─── Matchup tests ──────────────────────────────────────────────────────────

async fn create_url_submission(
    tx: &mut sqlx::PgConnection,
    round_id: Uuid,
    author_id: Uuid,
    url: &str,
) -> submissions::SubmissionRecord {
    submissions::create_submission(
        tx,
        round_id,
        author_id,
        submissions::ContentType::Url,
        Some(url),
        None,
        None,
    )
    .await
    .expect("create url submission")
}

#[shared_runtime_test]
async fn test_create_matchup_orders_pair() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let ranker = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("ranker");
    let author_a = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("author a");
    let author_b = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("author b");

    let sub_a = create_url_submission(&mut *tx, round.id, author_a.id, "https://a.com").await;
    let sub_b = create_url_submission(&mut *tx, round.id, author_b.id, "https://b.com").await;

    // Deliberately pass b, a (reversed) to verify the repo re-orders them.
    let (bigger, smaller) = if sub_a.id > sub_b.id {
        (sub_a.id, sub_b.id)
    } else {
        (sub_b.id, sub_a.id)
    };

    let record = matchups::create_matchup(
        &mut *tx, round.id, ranker.id, bigger, // intentionally the larger UUID first
        smaller, None,
    )
    .await
    .expect("create matchup");

    assert!(
        record.submission_a < record.submission_b,
        "submission_a must be < submission_b after ordering"
    );
}

#[shared_runtime_test]
async fn test_duplicate_matchup_fails() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let ranker = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("ranker");
    let auth_a = AccountFactory::new().create(&mut *tx).await.expect("a");
    let auth_b = AccountFactory::new().create(&mut *tx).await.expect("b");

    let sub_a = create_url_submission(&mut *tx, round.id, auth_a.id, "https://a.com").await;
    let sub_b = create_url_submission(&mut *tx, round.id, auth_b.id, "https://b.com").await;

    matchups::create_matchup(&mut *tx, round.id, ranker.id, sub_a.id, sub_b.id, None)
        .await
        .expect("first matchup");

    let result =
        matchups::create_matchup(&mut *tx, round.id, ranker.id, sub_a.id, sub_b.id, None).await;

    assert!(result.is_err(), "duplicate matchup should fail");
}

#[shared_runtime_test]
async fn test_get_judged_pairs_returns_correct_set() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let ranker = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("ranker");
    let auth_a = AccountFactory::new().create(&mut *tx).await.expect("a");
    let auth_b = AccountFactory::new().create(&mut *tx).await.expect("b");
    let auth_c = AccountFactory::new().create(&mut *tx).await.expect("c");

    let sub_a = create_url_submission(&mut *tx, round.id, auth_a.id, "https://a.com").await;
    let sub_b = create_url_submission(&mut *tx, round.id, auth_b.id, "https://b.com").await;
    let sub_c = create_url_submission(&mut *tx, round.id, auth_c.id, "https://c.com").await;

    matchups::create_matchup(
        &mut *tx,
        round.id,
        ranker.id,
        sub_a.id,
        sub_b.id,
        Some(sub_a.id),
    )
    .await
    .expect("matchup a-b");
    matchups::create_matchup(
        &mut *tx,
        round.id,
        ranker.id,
        sub_b.id,
        sub_c.id,
        Some(sub_b.id),
    )
    .await
    .expect("matchup b-c");

    let pairs = matchups::get_judged_pairs(&mut *tx, round.id, ranker.id)
        .await
        .expect("get judged pairs");

    assert_eq!(pairs.len(), 2);
    // All returned pairs must be ordered a < b.
    for (a, b) in &pairs {
        assert!(a < b, "judged pair must be ordered a < b");
    }
}

#[shared_runtime_test]
async fn test_count_matchups_for_ranker() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let ranker = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("ranker");
    let auth_a = AccountFactory::new().create(&mut *tx).await.expect("a");
    let auth_b = AccountFactory::new().create(&mut *tx).await.expect("b");
    let auth_c = AccountFactory::new().create(&mut *tx).await.expect("c");

    let sub_a = create_url_submission(&mut *tx, round.id, auth_a.id, "https://a.com").await;
    let sub_b = create_url_submission(&mut *tx, round.id, auth_b.id, "https://b.com").await;
    let sub_c = create_url_submission(&mut *tx, round.id, auth_c.id, "https://c.com").await;

    let c0 = matchups::count_matchups_for_ranker(&mut *tx, round.id, ranker.id)
        .await
        .expect("count 0");
    assert_eq!(c0, 0);

    matchups::create_matchup(&mut *tx, round.id, ranker.id, sub_a.id, sub_b.id, None)
        .await
        .expect("matchup 1");
    matchups::create_matchup(&mut *tx, round.id, ranker.id, sub_a.id, sub_c.id, None)
        .await
        .expect("matchup 2");

    let c2 = matchups::count_matchups_for_ranker(&mut *tx, round.id, ranker.id)
        .await
        .expect("count 2");
    assert_eq!(c2, 2);
}

// ─── Rating tests ────────────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_initialize_ratings_defaults() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let auth_a = AccountFactory::new().create(&mut *tx).await.expect("a");
    let auth_b = AccountFactory::new().create(&mut *tx).await.expect("b");

    let sub_a = create_url_submission(&mut *tx, round.id, auth_a.id, "https://a.com").await;
    let sub_b = create_url_submission(&mut *tx, round.id, auth_b.id, "https://b.com").await;

    // Use initialize_ratings with the transaction executor.
    ratings::initialize_ratings(&mut *tx, &[sub_a.id, sub_b.id])
        .await
        .expect("initialize ratings");

    let ra = ratings::get_rating(&mut *tx, sub_a.id)
        .await
        .expect("get rating a")
        .expect("rating a exists");

    assert!(
        (ra.rating - 1500.0).abs() < f64::EPSILON,
        "default rating is 1500"
    );
    assert!(
        (ra.deviation - 350.0).abs() < f64::EPSILON,
        "default deviation is 350"
    );
    assert!(
        (ra.volatility - 0.06).abs() < 1e-9,
        "default volatility is 0.06"
    );
    assert_eq!(ra.matchup_count, 0, "default matchup_count is 0");
}

#[shared_runtime_test]
async fn test_update_rating() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let author = AccountFactory::new()
        .create(&mut *tx)
        .await
        .expect("author");

    let sub = create_url_submission(&mut *tx, round.id, author.id, "https://a.com").await;

    sqlx::query("INSERT INTO rooms__ratings (submission_id) VALUES ($1)")
        .bind(sub.id)
        .execute(&mut *tx)
        .await
        .expect("insert rating");

    ratings::update_rating(&mut *tx, sub.id, 1600.0, 200.0, 0.07, 5)
        .await
        .expect("update rating");

    let r = ratings::get_rating(&mut *tx, sub.id)
        .await
        .expect("get rating")
        .expect("exists");

    assert!((r.rating - 1600.0).abs() < f64::EPSILON);
    assert!((r.deviation - 200.0).abs() < f64::EPSILON);
    assert!((r.volatility - 0.07).abs() < 1e-9);
    assert_eq!(r.matchup_count, 5);
}

#[shared_runtime_test]
async fn test_get_ratings_for_round_ordered_desc() {
    let mut tx = test_transaction().await;

    let room_id = create_test_room(&mut *tx).await;
    let round = create_test_round(&mut *tx, room_id, 1).await;
    let auth_a = AccountFactory::new().create(&mut *tx).await.expect("a");
    let auth_b = AccountFactory::new().create(&mut *tx).await.expect("b");
    let auth_c = AccountFactory::new().create(&mut *tx).await.expect("c");

    let sub_a = create_url_submission(&mut *tx, round.id, auth_a.id, "https://a.com").await;
    let sub_b = create_url_submission(&mut *tx, round.id, auth_b.id, "https://b.com").await;
    let sub_c = create_url_submission(&mut *tx, round.id, auth_c.id, "https://c.com").await;

    // Insert ratings with known values.
    sqlx::query(
        "INSERT INTO rooms__ratings (submission_id, rating) VALUES ($1, 1700), ($2, 1500), ($3, 1600)",
    )
    .bind(sub_a.id)
    .bind(sub_b.id)
    .bind(sub_c.id)
    .execute(&mut *tx)
    .await
    .expect("insert ratings");

    let list = ratings::get_ratings_for_round(&mut *tx, round.id)
        .await
        .expect("get ratings for round");

    assert_eq!(list.len(), 3);
    // Should be ordered by rating DESC.
    assert!(list[0].rating >= list[1].rating);
    assert!(list[1].rating >= list[2].rating);
    assert_eq!(
        list[0].submission_id, sub_a.id,
        "sub_a has highest rating (1700)"
    );
}

// ─── Hall of fame tests ──────────────────────────────────────────────────────

#[shared_runtime_test]
async fn test_insert_and_list_hall_of_fame() {
    // Use isolated_db so all writes are cleaned up automatically on drop.
    let db = common::test_db::isolated_db().await;
    let pool = db.pool();

    let room_id: Uuid = sqlx::query_scalar(
        "INSERT INTO rooms__rooms (name, status, engine_type, engine_config) \
         VALUES ($1, 'open', 'ranking', '{}') RETURNING id",
    )
    .bind(format!("hof-room-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("create hof room");

    let round_id: Uuid = sqlx::query_scalar(
        "INSERT INTO rooms__rounds \
         (room_id, round_number, submit_opens_at, rank_opens_at, closes_at) \
         VALUES ($1, 1, now(), now(), now()) RETURNING id",
    )
    .bind(room_id)
    .fetch_one(pool)
    .await
    .expect("create hof round");

    let author_id: Uuid = AccountFactory::new()
        .create(pool)
        .await
        .expect("hof author")
        .id;
    let sub_id: Uuid = sqlx::query_scalar(
        "INSERT INTO rooms__submissions (round_id, author_id, content_type, url) \
         VALUES ($1, $2, 'url', 'https://hof.com') RETURNING id",
    )
    .bind(round_id)
    .bind(author_id)
    .fetch_one(pool)
    .await
    .expect("create hof submission");

    let winners = vec![(sub_id, 1750.0f64, 1i32)];
    hall_of_fame::insert_winners(pool, room_id, round_id, &winners)
        .await
        .expect("insert winners");

    let list = hall_of_fame::list_hall_of_fame(pool, room_id, 10, 0)
        .await
        .expect("list hall of fame");

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].submission_id, sub_id);
    assert_eq!(list[0].room_id, room_id);
    assert_eq!(list[0].round_id, round_id);
    assert!((list[0].final_rating - 1750.0).abs() < f64::EPSILON);
    assert_eq!(list[0].rank, 1);
}

#[shared_runtime_test]
async fn test_hall_of_fame_pagination() {
    // Use isolated_db so all writes are cleaned up automatically on drop.
    let db = common::test_db::isolated_db().await;
    let pool = db.pool();

    let room_id: Uuid = sqlx::query_scalar(
        "INSERT INTO rooms__rooms (name, status, engine_type, engine_config) \
         VALUES ($1, 'open', 'ranking', '{}') RETURNING id",
    )
    .bind(format!("hof-page-room-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("create room");

    // Create 3 rounds with 1 submission each.
    for rn in 1..=3i32 {
        let round_id: Uuid = sqlx::query_scalar(
            "INSERT INTO rooms__rounds \
             (room_id, round_number, submit_opens_at, rank_opens_at, closes_at) \
             VALUES ($1, $2, now(), now(), now()) RETURNING id",
        )
        .bind(room_id)
        .bind(rn)
        .fetch_one(pool)
        .await
        .expect("create round");

        let author = AccountFactory::new().create(pool).await.expect("author");
        let sub_id: Uuid = sqlx::query_scalar(
            "INSERT INTO rooms__submissions (round_id, author_id, content_type, url) \
             VALUES ($1, $2, 'url', $3) RETURNING id",
        )
        .bind(round_id)
        .bind(author.id)
        .bind(format!("https://example.com/{rn}.png"))
        .fetch_one(pool)
        .await
        .expect("create submission");

        hall_of_fame::insert_winners(
            pool,
            room_id,
            round_id,
            &[(sub_id, 1500.0 + f64::from(rn) * 10.0, 1)],
        )
        .await
        .expect("insert winner");
    }

    let page1 = hall_of_fame::list_hall_of_fame(pool, room_id, 2, 0)
        .await
        .expect("page 1");
    assert_eq!(page1.len(), 2, "page 1 should have 2 entries");

    let page2 = hall_of_fame::list_hall_of_fame(pool, room_id, 2, 2)
        .await
        .expect("page 2");
    assert_eq!(page2.len(), 1, "page 2 should have 1 entry");
}
