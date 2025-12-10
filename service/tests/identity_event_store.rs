use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use uuid::Uuid;

use tinycongress_api::db;
use tinycongress_api::identity::crypto::{
    derive_kid, sign_message, EnvelopeSigner, SignedEnvelope,
};
use tinycongress_api::identity::repo::event_store::{
    append_signed_event, fetch_events, AppendEventInput,
};

const SECRET_KEY: [u8; 32] = [
    42, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];

async fn reset_identity_tables(pool: &PgPool) {
    sqlx::query("TRUNCATE TABLE signed_events, recovery_approvals, recovery_policies, device_delegations, devices, accounts CASCADE")
        .execute(pool)
        .await
        .unwrap();
}

async fn create_account(pool: &PgPool) -> Uuid {
    let account_id = Uuid::new_v4();
    let root_pubkey = vec![9u8; 32];
    let root_kid = derive_kid(&root_pubkey);

    sqlx::query(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(account_id)
    .bind(format!("user-{}", account_id.simple()))
    .bind(root_kid)
    .bind(URL_SAFE_NO_PAD.encode(root_pubkey))
    .execute(pool)
    .await
    .unwrap();

    account_id
}

fn make_envelope(account_id: Uuid, seqno: i64, prev_hash_b64: Option<String>) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&SECRET_KEY);
    let public_key = signing_key.verifying_key();
    let kid = derive_kid(&public_key.to_bytes());

    let payload = serde_json::json!({
        "seqno": seqno,
        "prev_hash": prev_hash_b64,
        "body": {"message": format!("event-{seqno}")}
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "TestEvent".to_string(),
        payload,
        signer: EnvelopeSigner {
            account_id: Some(account_id),
            device_id: None,
            kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, &SECRET_KEY).unwrap();
    envelope.sig = URL_SAFE_NO_PAD.encode(signature);
    envelope
}

#[tokio::test]
async fn append_events_enforces_prev_hash_and_seqno() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();
    reset_identity_tables(&pool).await;

    let account_id = create_account(&pool).await;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&SECRET_KEY);
    let public_key = signing_key.verifying_key();

    let first_envelope = make_envelope(account_id, 1, None);
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id,
            seqno: 1,
            event_type: "TestEvent".to_string(),
            envelope: first_envelope.clone(),
            signer_pubkey: public_key.as_bytes(),
        },
    )
    .await
    .expect("first append works");

    // Compute prev hash for second link
    let first_hash = Sha256::digest(
        &first_envelope
            .canonical_signing_bytes()
            .expect("canonical bytes for first"),
    );
    let prev_hash_b64 = URL_SAFE_NO_PAD.encode(first_hash);

    let second_envelope = make_envelope(account_id, 2, Some(prev_hash_b64));
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id,
            seqno: 2,
            event_type: "TestEvent".to_string(),
            envelope: second_envelope,
            signer_pubkey: public_key.as_bytes(),
        },
    )
    .await
    .expect("second append works");

    let events = fetch_events(&pool, account_id).await.unwrap();
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn reject_prev_hash_mismatch() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();
    reset_identity_tables(&pool).await;

    let account_id = create_account(&pool).await;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&SECRET_KEY);
    let public_key = signing_key.verifying_key();

    let first_envelope = make_envelope(account_id, 1, None);
    append_signed_event(
        &pool,
        AppendEventInput {
            account_id,
            seqno: 1,
            event_type: "TestEvent".to_string(),
            envelope: first_envelope,
            signer_pubkey: public_key.as_bytes(),
        },
    )
    .await
    .expect("first append works");

    let bad_prev_hash = URL_SAFE_NO_PAD.encode([1u8; 32]);
    let bad_envelope = make_envelope(account_id, 2, Some(bad_prev_hash));
    let err = append_signed_event(
        &pool,
        AppendEventInput {
            account_id,
            seqno: 2,
            event_type: "TestEvent".to_string(),
            envelope: bad_envelope,
            signer_pubkey: public_key.as_bytes(),
        },
    )
    .await
    .expect_err("should reject prev hash mismatch");

    assert!(err.to_string().contains("prev_hash"));
}
