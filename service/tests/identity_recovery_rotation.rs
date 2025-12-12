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

const MAIN_ROOT_SECRET: [u8; 32] = [10u8; 32];
const MAIN_DEVICE_SECRET: [u8; 32] = [11u8; 32];
const HELPER1_ROOT_SECRET: [u8; 32] = [12u8; 32];
const HELPER1_DEVICE_SECRET: [u8; 32] = [13u8; 32];
const HELPER2_ROOT_SECRET: [u8; 32] = [14u8; 32];
const HELPER2_DEVICE_SECRET: [u8; 32] = [15u8; 32];
const NEW_ROOT_SECRET: [u8; 32] = [16u8; 32];

fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pubkey_b64(secret: &[u8; 32]) -> String {
    let signing = ed25519_dalek::SigningKey::from_bytes(secret);
    encode(&signing.verifying_key().to_bytes())
}

fn build_delegation_envelope(
    account_id: Uuid,
    device_id: Uuid,
    root_secret: &[u8; 32],
    device_secret: &[u8; 32],
    seqno: i64,
    prev_hash: Option<&str>,
) -> SignedEnvelope {
    let root_signing = ed25519_dalek::SigningKey::from_bytes(root_secret);
    let root_kid = derive_kid(&root_signing.verifying_key().to_bytes());
    let device_pubkey_b64 = pubkey_b64(device_secret);

    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash,
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
            kid: root_kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, root_secret).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

#[allow(clippy::too_many_arguments)]
fn build_approval_envelope(
    helper_account_id: Uuid,
    helper_device_id: Uuid,
    seqno: i64,
    prev_hash: &str,
    policy_id: Uuid,
    new_root_pubkey_b64: &str,
    new_root_kid: &str,
    helper_device_secret: &[u8; 32],
) -> SignedEnvelope {
    let helper_signing = ed25519_dalek::SigningKey::from_bytes(helper_device_secret);
    let helper_device_kid = derive_kid(&helper_signing.verifying_key().to_bytes());

    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash,
        "policy_id": policy_id.to_string(),
        "new_root_kid": new_root_kid,
        "new_root_pubkey": new_root_pubkey_b64,
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "RecoveryApproval".to_string(),
        payload,
        signer: EnvelopeSigner {
            account_id: Some(helper_account_id),
            device_id: Some(helper_device_id),
            kid: helper_device_kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, helper_device_secret).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

fn build_rotation_envelope(
    account_id: Uuid,
    seqno: i64,
    prev_hash: &str,
    policy_id: Uuid,
    new_root_pubkey_b64: &str,
    new_root_kid: &str,
    new_root_secret: &[u8; 32],
) -> SignedEnvelope {
    let payload = json!({
        "seqno": seqno,
        "prev_hash": prev_hash,
        "policy_id": policy_id.to_string(),
        "new_root_kid": new_root_kid,
        "new_root_pubkey": new_root_pubkey_b64,
    });

    let mut envelope = SignedEnvelope {
        v: 1,
        payload_type: "RootRotation".to_string(),
        payload,
        signer: EnvelopeSigner {
            account_id: Some(account_id),
            device_id: None,
            kid: new_root_kid.to_string(),
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, new_root_secret).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

fn build_policy_envelope(
    account_id: Uuid,
    seqno: i64,
    prev_hash: &str,
    threshold: i32,
    helpers: &[serde_json::Value],
    root_secret: &[u8; 32],
) -> SignedEnvelope {
    let root_signing = ed25519_dalek::SigningKey::from_bytes(root_secret);
    let root_kid = derive_kid(&root_signing.verifying_key().to_bytes());

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
            kid: root_kid,
        },
        sig: String::new(),
    };

    let signing_bytes = envelope.canonical_signing_bytes().unwrap();
    let signature = sign_message(&signing_bytes, root_secret).unwrap();
    envelope.sig = encode(&signature);
    envelope
}

#[tokio::test]
async fn recovery_approvals_and_rotation_flow() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE TABLE recovery_approvals, recovery_policies, sessions, signed_events, endorsements, device_delegations, devices, accounts CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let helper1_account = Uuid::new_v4();
    let helper1_device = Uuid::new_v4();
    let helper2_account = Uuid::new_v4();
    let helper2_device = Uuid::new_v4();

    let new_root_pubkey_b64 = pubkey_b64(&NEW_ROOT_SECRET);
    let new_root_kid = derive_kid(
        &ed25519_dalek::SigningKey::from_bytes(&NEW_ROOT_SECRET)
            .verifying_key()
            .to_bytes(),
    );

    let app = http::router().layer(axum::Extension(pool.clone()));

    // create primary account
    let delegation = build_delegation_envelope(
        account_id,
        device_id,
        &MAIN_ROOT_SECRET,
        &MAIN_DEVICE_SECRET,
        1,
        None,
    );

    let signup_body = json!({
        "username": "rotate-me",
        "root_pubkey": pubkey_b64(&MAIN_ROOT_SECRET),
        "device_pubkey": pubkey_b64(&MAIN_DEVICE_SECRET),
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

    // helper accounts
    for (username, account, device, root_secret, device_secret) in [
        (
            "helper-one",
            helper1_account,
            helper1_device,
            HELPER1_ROOT_SECRET,
            HELPER1_DEVICE_SECRET,
        ),
        (
            "helper-two",
            helper2_account,
            helper2_device,
            HELPER2_ROOT_SECRET,
            HELPER2_DEVICE_SECRET,
        ),
    ] {
        let helper_delegation =
            build_delegation_envelope(account, device, &root_secret, &device_secret, 1, None);
        let helper_signup = json!({
            "username": username,
            "root_pubkey": pubkey_b64(&root_secret),
            "device_pubkey": pubkey_b64(&device_secret),
            "device_metadata": {"name": "helper", "type": "laptop"},
            "delegation_envelope": helper_delegation,
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/signup")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_vec(&helper_signup).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
    }

    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let prev_hash_b64 = encode(&last_hash);

    let policy_helpers = vec![
        json!({"helper_account_id": helper1_account.to_string(), "helper_root_kid": derive_kid(&ed25519_dalek::SigningKey::from_bytes(&HELPER1_ROOT_SECRET).verifying_key().to_bytes())}),
        json!({"helper_account_id": helper2_account.to_string(), "helper_root_kid": derive_kid(&ed25519_dalek::SigningKey::from_bytes(&HELPER2_ROOT_SECRET).verifying_key().to_bytes())}),
    ];
    let policy_envelope = build_policy_envelope(
        account_id,
        2,
        &prev_hash_b64,
        2,
        &policy_helpers,
        &MAIN_ROOT_SECRET,
    );

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
    assert_eq!(policy_resp.status(), 200);
    let policy_json: serde_json::Value = serde_json::from_slice(
        &to_bytes(policy_resp.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let policy_id = Uuid::parse_str(policy_json["policy_id"].as_str().unwrap()).unwrap();

    // helper 1 approval
    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let helper1_prev_hash = encode(&last_hash);

    let approval1 = build_approval_envelope(
        helper1_account,
        helper1_device,
        3,
        &helper1_prev_hash,
        policy_id,
        &new_root_pubkey_b64,
        &new_root_kid,
        &HELPER1_DEVICE_SECRET,
    );

    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recovery/approve")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "helper_account_id": helper1_account,
                        "helper_device_id": helper1_device,
                        "policy_id": policy_id,
                        "envelope": approval1,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), 200);

    // helper 2 approval
    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let helper2_prev_hash = encode(&last_hash);
    let approval2 = build_approval_envelope(
        helper2_account,
        helper2_device,
        4,
        &helper2_prev_hash,
        policy_id,
        &new_root_pubkey_b64,
        &new_root_kid,
        &HELPER2_DEVICE_SECRET,
    );

    let approval_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recovery/approve")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "helper_account_id": helper2_account,
                        "helper_device_id": helper2_device,
                        "policy_id": policy_id,
                        "envelope": approval2,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approval_resp.status(), 200);

    // rotate root
    let last_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT canonical_bytes_hash FROM signed_events WHERE account_id = $1 ORDER BY seqno DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let rotation_prev_hash = encode(&last_hash);
    let rotation_envelope = build_rotation_envelope(
        account_id,
        5,
        &rotation_prev_hash,
        policy_id,
        &new_root_pubkey_b64,
        &new_root_kid,
        &NEW_ROOT_SECRET,
    );

    let rotation_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recovery/rotate_root")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "account_id": account_id,
                        "envelope": rotation_envelope,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotation_resp.status(), 200);

    let (root_kid_db,): (String,) = sqlx::query_as("SELECT root_kid FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(root_kid_db, new_root_kid);

    let (active_delegations, revoked_delegations): (i64, i64) = sqlx::query_as(
        "SELECT
            COUNT(*) FILTER (WHERE revoked_at IS NULL) AS active,
            COUNT(*) FILTER (WHERE revoked_at IS NOT NULL) AS revoked
        FROM device_delegations WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_delegations, 0);
    assert_eq!(revoked_delegations, 1);

    let (approvals_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM recovery_approvals WHERE account_id = $1 AND policy_id = $2",
    )
    .bind(account_id)
    .bind(policy_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(approvals_count, 2);

    let (event_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM signed_events WHERE account_id = $1")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(event_count, 5);
}
