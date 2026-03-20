//! Integration tests for trust score snapshot repository operations.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_engine_api::trust::TrustGraphReader;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::graph_reader::TrustRepoGraphReader;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};

#[shared_runtime_test]
async fn test_upsert_score_global() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(70)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool);
    repo.upsert_score(user.id, None, Some(0.0), Some(3), Some(0.8))
        .await
        .expect("upsert_score global");

    let score = repo
        .get_score(user.id, None)
        .await
        .expect("get_score")
        .expect("score should exist");

    assert_eq!(score.user_id, user.id);
    assert!(score.context_user_id.is_none());
    assert!((score.trust_distance.unwrap() - 0.0).abs() < f32::EPSILON);
    assert_eq!(score.path_diversity, Some(3));
    assert!((score.eigenvector_centrality.unwrap() - 0.8).abs() < 1e-5);
}

#[shared_runtime_test]
async fn test_upsert_score_with_context() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(71)
        .create(&pool)
        .await
        .expect("create user");

    let ctx = AccountFactory::new()
        .with_seed(72)
        .create(&pool)
        .await
        .expect("create context user");

    let repo = PgTrustRepo::new(pool);
    repo.upsert_score(user.id, Some(ctx.id), Some(2.0), Some(1), Some(0.5))
        .await
        .expect("upsert_score with context");

    let score = repo
        .get_score(user.id, Some(ctx.id))
        .await
        .expect("get_score")
        .expect("score should exist");

    assert_eq!(score.user_id, user.id);
    assert_eq!(score.context_user_id, Some(ctx.id));
    assert!((score.trust_distance.unwrap() - 2.0).abs() < f32::EPSILON);
}

#[shared_runtime_test]
async fn test_upsert_score_updates_on_conflict() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(73)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool);

    // First upsert
    repo.upsert_score(user.id, None, Some(1.0), Some(2), Some(0.3))
        .await
        .expect("first upsert");

    // Second upsert — should overwrite
    repo.upsert_score(user.id, None, Some(5.0), Some(7), Some(0.9))
        .await
        .expect("second upsert");

    let score = repo
        .get_score(user.id, None)
        .await
        .expect("get_score")
        .expect("score should exist");

    assert!((score.trust_distance.unwrap() - 5.0).abs() < f32::EPSILON);
    assert_eq!(score.path_diversity, Some(7));
    assert!((score.eigenvector_centrality.unwrap() - 0.9).abs() < 1e-5);
}

#[shared_runtime_test]
async fn test_get_score_not_found_returns_none() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(74)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool);
    let score = repo
        .get_score(user.id, None)
        .await
        .expect("get_score should not error");

    assert!(score.is_none());
}

#[shared_runtime_test]
async fn test_get_all_scores_returns_multiple_contexts() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(75)
        .create(&pool)
        .await
        .expect("create user");

    let ctx1 = AccountFactory::new()
        .with_seed(76)
        .create(&pool)
        .await
        .expect("create ctx1");

    let ctx2 = AccountFactory::new()
        .with_seed(77)
        .create(&pool)
        .await
        .expect("create ctx2");

    let repo = PgTrustRepo::new(pool);

    repo.upsert_score(user.id, None, Some(0.0), None, Some(0.5))
        .await
        .expect("global score");
    repo.upsert_score(user.id, Some(ctx1.id), Some(1.0), Some(2), None)
        .await
        .expect("ctx1 score");
    repo.upsert_score(user.id, Some(ctx2.id), Some(3.0), Some(1), None)
        .await
        .expect("ctx2 score");

    let all = repo.get_all_scores(user.id).await.expect("get_all_scores");

    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|s| s.user_id == user.id));
}

// ---------------------------------------------------------------------------
// Test 6: graph reader — negative path_diversity treated as no score
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_graph_reader_treats_negative_path_diversity_as_no_score() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(78)
        .create(&pool)
        .await
        .expect("create user");

    // Insert a score with negative path_diversity directly — simulates data corruption.
    // The column is INTEGER so -1 is representable even though the engine never writes it.
    sqlx::query(
        "INSERT INTO trust__score_snapshots \
         (user_id, context_user_id, trust_distance, path_diversity, eigenvector_centrality) \
         VALUES ($1, NULL, 1.0, -1, 0.5)",
    )
    .bind(user.id)
    .execute(&pool)
    .await
    .expect("insert corrupted score");

    let trust_repo = Arc::new(PgTrustRepo::new(pool));
    let reader = TrustRepoGraphReader::new(trust_repo);
    let score = reader
        .get_score(user.id, None)
        .await
        .expect("get_score should not error even on corrupt data");

    assert!(
        score.is_none(),
        "negative path_diversity should be treated as no score (data corruption guard)"
    );
}
