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

const ROOT_SECRET_KEY: [u8; 32] = [5u8; 32];
const DEVICE_SECRET_KEY: [u8; 32] = [6u8; 32];

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

fn build_policy_envelope(
    account_id: Uuid,
    prev_hash: &str,
    seqno: i64,
    threshold: i32,
    helpers: &[serde_json::Value],
) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash,
        "threshold": threshold,
        "helpers": helpers,
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "RecoveryPolicy".to_string(),
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

fn build_revocation_envelope(account_id: Uuid, prev_hash: &str, seqno: i64) -> SignedEnvelope {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
    let root_pubkey = signing_key.verifying_key();
    let kid = derive_kid(&root_pubkey.to_bytes());

    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash,
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "RecoveryPolicyRevocation".to_string(),
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
async fn recovery_policy_create_and_revoke() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query(
        "TRUNCATE TABLE recovery_approvals, recovery_policies, sessions, signed_events, endorsements, device_delegations, devices, accounts CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let helper_one = Uuid::new_v4();
    let helper_two = Uuid::new_v4();

    let root_pubkey_b64 = {
        let signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
        encode(&signing.verifying_key().to_bytes())
    };

    let delegation = build_delegation_envelope(account_id, device_id);
    let app = http::router().layer(axum::Extension(pool.clone()));

    let signup_body = json!({
        "username": "recoverable",
        "root_pubkey": root_pubkey_b64,
        "device_pubkey": encode(&ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY).verifying_key().to_bytes()),
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
    let signup_status = signup_resp.status();
    let signup_body = to_bytes(signup_resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(
        signup_status == 200,
        "signup failed: {} {}",
        signup_status,
        String::from_utf8_lossy(&signup_body)
    );

    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let prev_hash_b64 = encode(&last_hash);

    let helpers = vec![
        json!({"helper_account_id": helper_one.to_string(), "helper_root_kid": "helper-one"}),
        json!({"helper_account_id": helper_two.to_string(), "helper_root_kid": null}),
    ];
    let policy_envelope = build_policy_envelope(account_id, &prev_hash_b64, 2, 2, &helpers);

    let policy_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/me/recovery_policy")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "envelope": policy_envelope,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let policy_status = policy_resp.status();
    let policy_body = to_bytes(policy_resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert!(
        policy_status.is_success(),
        "policy creation failed: {} {}",
        policy_status,
        String::from_utf8_lossy(&policy_body)
    );
    let policy_json: serde_json::Value = serde_json::from_slice(&policy_body).unwrap();
    let policy_id = Uuid::parse_str(policy_json["policy_id"].as_str().unwrap()).expect("policy id");

    let (stored_thresh, active_count): (i32, i64) = sqlx::query_as(
        "SELECT threshold, COUNT(*) OVER () FROM recovery_policies WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_thresh, 2);
    assert_eq!(active_count, 1);

    let get_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/me/recovery_policy?account_id={account_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);
    let get_json: serde_json::Value =
        serde_json::from_slice(&to_bytes(get_resp.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(
        get_json["policy_id"].as_str().unwrap(),
        policy_id.to_string()
    );
    assert_eq!(get_json["helpers"].as_array().unwrap().len(), 2);

    let last_event: (Vec<u8>, i64) = sqlx::query_as(
        "SELECT canonical_bytes_hash, seqno FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let revoke_envelope =
        build_revocation_envelope(account_id, &encode(&last_event.0), last_event.1 + 1);

    let revoke_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/me/recovery_policy")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "envelope": revoke_envelope,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), 204);

    let (revoked_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM recovery_policies WHERE account_id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revoked_count, 1);

    let missing_resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/me/recovery_policy?account_id={account_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_resp.status(), 404);

    let (event_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM signed_events WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 3);
}

#[tokio::test]
async fn reject_threshold_over_helpers() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query(
        "TRUNCATE TABLE recovery_approvals, recovery_policies, sessions, signed_events, endorsements, device_delegations, devices, accounts CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let root_pubkey_b64 = {
        let signing = ed25519_dalek::SigningKey::from_bytes(&ROOT_SECRET_KEY);
        encode(&signing.verifying_key().to_bytes())
    };

    let delegation = build_delegation_envelope(account_id, device_id);
    let app = http::router().layer(axum::Extension(pool.clone()));

    let signup_body = json!({
        "username": "recoverable-invalid",
        "root_pubkey": root_pubkey_b64,
        "device_pubkey": encode(&ed25519_dalek::SigningKey::from_bytes(&DEVICE_SECRET_KEY).verifying_key().to_bytes()),
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

    let helpers = vec![json!({"helper_account_id": Uuid::new_v4().to_string()})];
    let policy_envelope = build_policy_envelope(account_id, &prev_hash_b64, 2, 2, &helpers);

    let policy_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/me/recovery_policy")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "envelope": policy_envelope,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(policy_resp.status(), 422);

    let (policy_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM recovery_policies WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(policy_count, 0);
}
