#![allow(clippy::too_many_lines)]

use axum::{body::Body, http::Request};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

use tinycongress_api::db;
use tinycongress_api::identity::crypto::{
    derive_kid, sign_message, EnvelopeSigner, SignedEnvelope,
};
use tinycongress_api::identity::http;

const ROOT_SECRET_KEY: [u8; 32] = [11u8; 32];
const DEVICE_SECRET_KEY: [u8; 32] = [12u8; 32];

fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn build_delegation_envelope(
    account_id: Uuid,
    device_id: Uuid,
    device_pubkey_b64: &str,
    seqno: i64,
    prev_hash: Option<&str>,
) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash.map(str::to_string),
        "device_id": device_id.to_string(),
        "device_pubkey": device_pubkey_b64,
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

fn build_revocation_envelope(
    account_id: Uuid,
    device_id: Uuid,
    seqno: i64,
    prev_hash: &str,
) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash,
        "device_id": device_id.to_string(),
        "revocation": true,
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "DeviceRevocation".to_string(),
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

#[tokio::test]
async fn revoke_device_marks_device_and_delegation() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE TABLE signed_events, device_delegations, devices, accounts CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let root_signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey_b64 = encode(&root_signing.verifying_key().to_bytes());

    let device_signing = ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY);
    let device_pubkey_b64 = encode(&device_signing.verifying_key().to_bytes());

    let delegation = build_delegation_envelope(account_id, device_id, &device_pubkey_b64, 1, None);
    let first_hash = {
        let canonical = delegation
            .clone()
            .canonical_signing_bytes()
            .expect("canonical bytes");
        let digest = Sha256::digest(canonical);
        encode(&digest)
    };

    let app = http::router().layer(axum::Extension(pool.clone()));

    // Seed signup to create device and first sigchain entry
    let signup_body = json!({
        "username": "revokeuser",
        "root_pubkey": root_pubkey_b64,
        "device_pubkey": device_pubkey_b64,
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

    // Prepare revocation with prev_hash of first link
    let revocation_envelope = build_revocation_envelope(account_id, device_id, 2, &first_hash);

    let revoke_body = json!({
        "account_id": account_id,
        "delegation_envelope": revocation_envelope,
        "reason": "lost device",
    });

    let revoke_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/me/devices/{device_id}/revoke"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&revoke_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), 204);

    // Device marked revoked
    let (revoked_at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT revoked_at FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(revoked_at.is_some());

    // Delegation revoked
    let (delegation_revoked_at,): (Option<chrono::DateTime<chrono::Utc>>,) = sqlx::query_as(
        "SELECT revoked_at FROM device_delegations WHERE device_id = $1 AND account_id = $2",
    )
    .bind(device_id)
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(delegation_revoked_at.is_some());

    // Sigchain advanced
    let (event_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM signed_events WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 2);
}
