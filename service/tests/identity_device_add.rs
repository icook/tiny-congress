#![allow(clippy::too_many_lines)]

use axum::body::to_bytes;
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

const ROOT_SECRET_KEY: [u8; 32] = [9u8; 32];
const FIRST_DEVICE_SECRET_KEY: [u8; 32] = [8u8; 32];
const SECOND_DEVICE_SECRET_KEY: [u8; 32] = [7u8; 32];

fn encode_key_bytes(bytes: &[u8]) -> String {
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
    envelope.sig = encode_key_bytes(&signature);
    envelope
}

#[tokio::test]
async fn add_device_appends_sigchain_and_persists() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE TABLE signed_events, device_delegations, devices, accounts CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let first_device_id = Uuid::new_v4();
    let second_device_id = Uuid::new_v4();

    let root_signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey_b64 = encode_key_bytes(&root_signing.verifying_key().to_bytes());

    let first_device_signing = ed25519_dalek::SigningKey::from_bytes(&FIRST_DEVICE_SECRET_KEY);
    let first_device_pubkey_b64 =
        encode_key_bytes(&first_device_signing.verifying_key().to_bytes());
    let first_envelope = build_delegation_envelope(
        account_id,
        first_device_id,
        &first_device_pubkey_b64,
        1,
        None,
    );

    let app = http::router().layer(axum::Extension(pool.clone()));

    let signup_request = json!({
        "username": "multidevice",
        "root_pubkey": root_pubkey_b64,
        "device_pubkey": first_device_pubkey_b64,
        "device_metadata": {"name": "laptop", "type": "laptop"},
        "delegation_envelope": first_envelope,
    });

    let signup_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/signup")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&signup_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signup_response.status(), 200);

    let prev_hash = {
        let canonical = first_envelope.canonical_signing_bytes().unwrap();
        let digest = Sha256::digest(canonical);
        encode_key_bytes(&digest)
    };

    let second_device_signing = ed25519_dalek::SigningKey::from_bytes(&SECOND_DEVICE_SECRET_KEY);
    let second_device_pubkey_b64 =
        encode_key_bytes(&second_device_signing.verifying_key().to_bytes());
    let second_envelope = build_delegation_envelope(
        account_id,
        second_device_id,
        &second_device_pubkey_b64,
        2,
        Some(&prev_hash),
    );

    let add_request = json!({
        "account_id": account_id,
        "device_pubkey": second_device_pubkey_b64,
        "device_metadata": {"name": "phone", "type": "phone"},
        "delegation_envelope": second_envelope,
    });

    let add_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/me/devices/add")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&add_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(add_response.status(), 200);

    let add_body = to_bytes(add_response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let add_json: serde_json::Value = serde_json::from_slice(&add_body).unwrap();
    assert_eq!(
        add_json.get("device_id").unwrap().as_str().unwrap(),
        second_device_id.to_string()
    );

    let expected_kid = derive_kid(&second_device_signing.verifying_key().to_bytes());
    assert_eq!(
        add_json.get("device_kid").unwrap().as_str().unwrap(),
        expected_kid
    );

    let (stored_kid,): (String,) = sqlx::query_as("SELECT device_kid FROM devices WHERE id = $1")
        .bind(second_device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_kid, expected_kid);

    let (delegations,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_delegations WHERE device_id = $1")
            .bind(second_device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(delegations, 1);

    let (event_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM signed_events WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 2);
}
