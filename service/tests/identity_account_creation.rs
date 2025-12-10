use axum::body::to_bytes;
use axum::{body::Body, http::Request};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

use tinycongress_api::db;
use tinycongress_api::identity::crypto::{derive_kid, sign_message, SignedEnvelope};
use tinycongress_api::identity::http;

const ROOT_SECRET_KEY: [u8; 32] = [1u8; 32];
const DEVICE_SECRET_KEY: [u8; 32] = [2u8; 32];

fn encode_key_bytes(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn build_delegation_envelope(
    account_id: Uuid,
    device_id: Uuid,
    device_pubkey_b64: &str,
) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": 1,
        "prev_hash": null,
        "device_id": device_id.to_string(),
        "device_pubkey": device_pubkey_b64,
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "DeviceDelegation".to_string(),
        payload,
        signer: tinycongress_api::identity::crypto::EnvelopeSigner {
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
async fn signup_creates_account_and_device() {
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
    let root_pubkey_b64 = encode_key_bytes(&root_signing.verifying_key().to_bytes());

    let device_signing = ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY);
    let device_pubkey_b64 = encode_key_bytes(&device_signing.verifying_key().to_bytes());

    let envelope = build_delegation_envelope(account_id, device_id, &device_pubkey_b64);

    let app = http::router().layer(axum::Extension(pool.clone()));

    let request_body = json!({
        "username": "newuser",
        "root_pubkey": root_pubkey_b64,
        "device_pubkey": device_pubkey_b64,
        "device_metadata": {"name": "laptop", "type": "laptop"},
        "delegation_envelope": envelope,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/signup")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let resp_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let returned_account = resp_json.get("account_id").unwrap().as_str().unwrap();
    let returned_device = resp_json.get("device_id").unwrap().as_str().unwrap();
    let returned_kid = resp_json.get("root_kid").unwrap().as_str().unwrap();

    assert_eq!(returned_account, account_id.to_string());
    assert_eq!(returned_device, device_id.to_string());
    assert_eq!(
        returned_kid,
        derive_kid(&root_signing.verifying_key().to_bytes())
    );

    // Check DB rows exist
    let (username,): (String,) = sqlx::query_as("SELECT username FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(username, "newuser");

    let (stored_device_id,): (Uuid,) = sqlx::query_as("SELECT id FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_device_id, device_id);

    let delegation_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM device_delegations WHERE device_id = $1")
            .bind(device_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(delegation_count.0, 1);
}
