//! Integration tests for evidence repo operations.

mod common;

use common::test_db::test_transaction;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::rooms::repo::{
    evidence::{
        delete_evidence_for_poll, get_evidence_for_dimensions, insert_evidence, NewEvidence,
    },
    polls, rooms,
};

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Create a room, poll, and dimension, returning (poll_id, dimension_id).
async fn setup_poll_with_dimension(
    conn: &mut sqlx::PgConnection,
    room_name: &str,
) -> (uuid::Uuid, uuid::Uuid) {
    let room = rooms::create_room(
        &mut *conn,
        room_name,
        None,
        "identity_verified",
        None,
        "open",
        &serde_json::json!({}),
        None,
    )
    .await
    .expect("create room");

    let poll = polls::create_poll(&mut *conn, room.id, "Should we do X?", None, None)
        .await
        .expect("create poll");

    let dimension = polls::create_dimension(
        &mut *conn,
        poll.id,
        "Effectiveness",
        None,
        0.0,
        10.0,
        0,
        None,
        None,
    )
    .await
    .expect("create dimension");

    (poll.id, dimension.id)
}

// ─── Tests ────────────────────────────────────────────────────────────────

/// insert_evidence returns the correct row count.
#[shared_runtime_test]
async fn test_insert_evidence_returns_count() {
    let mut tx = test_transaction().await;

    let (_poll_id, dimension_id) =
        setup_poll_with_dimension(&mut *tx, "Evidence Insert Count Room").await;

    let evidence = vec![
        NewEvidence {
            stance: "pro",
            claim: "It is effective",
            source: Some("https://example.com/1"),
        },
        NewEvidence {
            stance: "con",
            claim: "It is too expensive",
            source: None,
        },
    ];

    let count = insert_evidence(&mut *tx, dimension_id, &evidence)
        .await
        .expect("insert evidence");

    assert_eq!(count, 2, "expected 2 rows inserted");
}

/// get_evidence_for_dimensions returns inserted records with correct fields.
#[shared_runtime_test]
async fn test_get_evidence_for_dimensions() {
    let mut tx = test_transaction().await;

    let (_poll_id, dimension_id) = setup_poll_with_dimension(&mut *tx, "Evidence Query Room").await;

    let evidence = vec![
        NewEvidence {
            stance: "pro",
            claim: "Very effective approach",
            source: Some("https://example.com/source"),
        },
        NewEvidence {
            stance: "con",
            claim: "High cost barrier",
            source: None,
        },
    ];

    insert_evidence(&mut *tx, dimension_id, &evidence)
        .await
        .expect("insert evidence");

    let records = get_evidence_for_dimensions(&mut *tx, &[dimension_id])
        .await
        .expect("get evidence");

    assert_eq!(records.len(), 2);

    // ORDER BY stance DESC — "pro" > "con" alphabetically, so "pro" comes first
    let pro = records
        .iter()
        .find(|r| r.stance == "pro")
        .expect("pro record");
    assert_eq!(pro.dimension_id, dimension_id);
    assert_eq!(pro.claim, "Very effective approach");
    assert_eq!(pro.source.as_deref(), Some("https://example.com/source"));

    let con = records
        .iter()
        .find(|r| r.stance == "con")
        .expect("con record");
    assert_eq!(con.dimension_id, dimension_id);
    assert_eq!(con.claim, "High cost barrier");
    assert!(con.source.is_none());
}

/// get_evidence_for_dimensions returns empty vec for unknown dimension IDs.
#[shared_runtime_test]
async fn test_get_evidence_for_dimensions_empty() {
    let mut tx = test_transaction().await;

    let unknown_id = uuid::Uuid::new_v4();
    let records = get_evidence_for_dimensions(&mut *tx, &[unknown_id])
        .await
        .expect("get evidence");

    assert!(records.is_empty());
}

/// delete_evidence_for_poll removes all evidence for the poll's dimensions.
#[shared_runtime_test]
async fn test_delete_evidence_for_poll() {
    let mut tx = test_transaction().await;

    let (poll_id, dimension_id) = setup_poll_with_dimension(&mut *tx, "Evidence Delete Room").await;

    let evidence = vec![
        NewEvidence {
            stance: "pro",
            claim: "Claim A",
            source: None,
        },
        NewEvidence {
            stance: "con",
            claim: "Claim B",
            source: None,
        },
    ];

    insert_evidence(&mut *tx, dimension_id, &evidence)
        .await
        .expect("insert evidence");

    // Verify rows are there
    let before = get_evidence_for_dimensions(&mut *tx, &[dimension_id])
        .await
        .expect("get before");
    assert_eq!(before.len(), 2);

    // Delete
    let deleted = delete_evidence_for_poll(&mut *tx, poll_id)
        .await
        .expect("delete evidence");
    assert_eq!(deleted, 2, "expected 2 rows deleted");

    // Verify empty
    let after = get_evidence_for_dimensions(&mut *tx, &[dimension_id])
        .await
        .expect("get after");
    assert!(after.is_empty(), "expected no evidence after delete");
}
