#![allow(clippy::too_many_lines)]

use axum::{body::to_bytes, body::Body, http::Request};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

use tinycongress_api::db;
use tinycongress_api::identity::crypto::{
    canonicalize_value, derive_kid, sign_message, EnvelopeSigner, SignedEnvelope,
};
use tinycongress_api::identity::http;

const ROOT_SECRET_KEY: [u8; 32] = [21u8; 32];
const DEVICE_SECRET_KEY: [u8; 32] = [22u8; 32];

fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn build_delegation_envelope(account_id: Uuid, device_id: Uuid) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": 1,
        "prev_hash": null,
        "device_id": device_id.to_string(),
        "device_pubkey": encode(&ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY).verifying_key().to_bytes()),
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

#[tokio::test]
async fn challenge_and_verify_login_succeeds() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query(
        "TRUNCATE TABLE sessions, signed_events, device_delegations, devices, accounts CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let root_signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey_b64 = encode(&root_signing.verifying_key().to_bytes());
    let device_pubkey_b64 = encode(
        &ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY)
            .verifying_key()
            .to_bytes(),
    );

    let delegation = build_delegation_envelope(account_id, device_id);
    let app = http::router().layer(axum::Extension(pool.clone()));

    // Signup to seed account/device
    let signup_body = json!({
        "username": "loginuser",
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

    // Issue challenge
    let challenge_body = json!({"account_id": account_id, "device_id": device_id});
    let challenge_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/challenge")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&challenge_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(challenge_resp.status(), 200);
    let challenge_json: serde_json::Value = serde_json::from_slice(
        &to_bytes(challenge_resp.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let challenge_id = Uuid::parse_str(challenge_json["challenge_id"].as_str().unwrap()).unwrap();
    let nonce = challenge_json["nonce"].as_str().unwrap();

    // Sign verification payload with device key
    let verify_payload = json!({
        "challenge_id": challenge_id,
        "nonce": nonce,
        "account_id": account_id,
        "device_id": device_id,
    });
    let canonical = canonicalize_value(&verify_payload).unwrap();
    let signature =
        sign_message(&canonical, &DEVICE_SECRET_KEY).expect("device signs verification payload");
    let signature_b64 = encode(&signature);

    let verify_body = json!({
        "challenge_id": challenge_id,
        "account_id": account_id,
        "device_id": device_id,
        "signature": signature_b64,
    });

    let verify_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_resp.status(), 200);
    let verify_json: serde_json::Value = serde_json::from_slice(
        &to_bytes(verify_resp.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        verify_json["session_id"].as_str().unwrap(),
        challenge_id.to_string()
    );
}

#[tokio::test]
async fn challenge_rejected_for_revoked_device() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query(
        "TRUNCATE TABLE sessions, signed_events, device_delegations, devices, accounts CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let root_signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey_b64 = encode(&root_signing.verifying_key().to_bytes());
    let device_pubkey_b64 = encode(
        &ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY)
            .verifying_key()
            .to_bytes(),
    );

    let delegation = build_delegation_envelope(account_id, device_id);
    let app = http::router().layer(axum::Extension(pool.clone()));

    // Signup
    let signup_body = json!({
        "username": "revokeduser",
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

    // Mark device revoked
    sqlx::query("UPDATE devices SET revoked_at = NOW() WHERE id = $1")
        .bind(device_id)
        .execute(&pool)
        .await
        .unwrap();

    let challenge_body = json!({"account_id": account_id, "device_id": device_id});
    let challenge_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/challenge")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&challenge_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(challenge_resp.status(), 403);
}
