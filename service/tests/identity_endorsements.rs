#![allow(clippy::too_many_lines)]

use axum::{body::to_bytes, body::Body, http::Request};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

use tinycongress_api::db;
use tinycongress_api::identity::crypto::{
    derive_kid, sign_message, EnvelopeSigner, SignedEnvelope,
};
use tinycongress_api::identity::http;

const ROOT_SECRET_KEY: [u8; 32] = [31u8; 32];
const DEVICE_SECRET_KEY: [u8; 32] = [32u8; 32];

fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn device_pubkey_b64() -> String {
    let signing = ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY);
    encode(&signing.verifying_key().to_bytes())
}

fn build_delegation_envelope(account_id: Uuid, device_id: Uuid) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": 1,
        "prev_hash": null,
        "device_id": device_id.to_string(),
        "device_pubkey": device_pubkey_b64(),
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "DeviceDelegation".to_string(),
        payload,
        signer: EnvelopeSigner {
            account_id: Some(account_id),
            device_id: None,
            kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, &ROOT_SECRET_KEY).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

fn build_endorsement_envelope(
    account_id: Uuid,
    device_id: Uuid,
    prev_hash: Option<&str>,
    topic: &str,
) -> SignedEnvelope {
    let device_signing = ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY);
    let device_pubkey = device_signing.verifying_key();
    let kid = derive_kid(&device_pubkey.to_bytes());

    let payload = json!({
        "seqno": prev_hash.map_or(2, |_| 2),
        "prev_hash": prev_hash,
        "subject_type": "account",
        "subject_id": account_id.to_string(),
        "topic": topic,
        "magnitude": 0.8,
        "confidence": 0.9,
        "context": "Great collaborator",
        "tags": ["trustworthy"],
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "Endorsement".to_string(),
        payload,
        signer: EnvelopeSigner {
            account_id: Some(account_id),
            device_id: Some(device_id),
            kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, &DEVICE_SECRET_KEY).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

fn build_revocation_envelope(
    account_id: Uuid,
    device_id: Uuid,
    prev_hash: &str,
    endorsement_id: Uuid,
) -> SignedEnvelope {
    let device_signing = ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY);
    let device_pubkey = device_signing.verifying_key();
    let kid = derive_kid(&device_pubkey.to_bytes());

    let payload = json!({
        "seqno": 3,
        "prev_hash": prev_hash,
        "endorsement_id": endorsement_id.to_string(),
        "reason": "withdrawn"
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "EndorsementRevocation".to_string(),
        payload,
        signer: EnvelopeSigner {
            account_id: Some(account_id),
            device_id: Some(device_id),
            kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, &DEVICE_SECRET_KEY).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

#[tokio::test]
async fn endorsement_create_and_revoke() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE TABLE sessions, signed_events, endorsements, device_delegations, devices, accounts CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let root_signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey_b64 = encode(&root_signing.verifying_key().to_bytes());

    let delegation = build_delegation_envelope(account_id, device_id);
    let app = http::router().layer(axum::Extension(pool.clone()));

    let signup_body = json!({
        "username": "endorser",
        "root_pubkey": root_pubkey_b64,
        "device_pubkey": device_pubkey_b64(),
        "device_metadata": {"name": "primary", "type": "laptop"},
        "delegation_envelope": delegation,
    });

    let signup_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/signup")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&signup_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signup_resp.status(), 200);

    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let prev_hash_b64 = encode(&last_hash);

    let endorsement_envelope =
        build_endorsement_envelope(account_id, device_id, Some(&prev_hash_b64), "trustworthy");

    let create_body = json!({
        "account_id": account_id,
        "device_id": device_id,
        "envelope": endorsement_envelope,
    });
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/endorsements")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 200);
    let create_json: serde_json::Value = serde_json::from_slice(
        &to_bytes(create_resp.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let endorsement_id = Uuid::parse_str(create_json["endorsement_id"].as_str().unwrap()).unwrap();

    let (stored_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM endorsements WHERE id = $1 AND revoked_at IS NULL")
            .bind(endorsement_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_count, 1);

    // Build revocation with prev_hash of endorsement creation event
    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let prev_hash_b64 = encode(&last_hash);

    let revoke_envelope =
        build_revocation_envelope(account_id, device_id, &prev_hash_b64, endorsement_id);
    let revoke_body = json!({
        "account_id": account_id,
        "device_id": device_id,
        "envelope": revoke_envelope,
    });

    let revoke_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/endorsements/{endorsement_id}/revoke"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&revoke_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), 204);

    let (revoked_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM endorsements WHERE id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(endorsement_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revoked_count, 1);

    let (event_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM signed_events WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 3);
}
