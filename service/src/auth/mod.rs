mod google;
mod state;

use crate::config::AppConfig;
use chrono::{DateTime, Utc};
pub use google::{GoogleOAuthProvider, GoogleUserInfo};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx_core::{query::query, row::Row};
use sqlx_postgres::{PgPool, PgRow};
pub use state::OAuthStateStore;
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_STATE_TTL: Duration = Duration::from_secs(300);
const SESSION_TTL: Duration = Duration::from_secs(60 * 15);

#[derive(Clone, Debug)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub display_name: Option<String>,
}

#[derive(Clone)]
pub struct OAuthService {
    pub state_store: OAuthStateStore,
    pub google: Option<GoogleOAuthProvider>,
}

impl OAuthService {
    pub fn from_config(config: &AppConfig) -> Self {
        let google = config.google_oauth.as_ref().and_then(|cfg| {
            GoogleOAuthProvider::new(cfg)
                .map_err(|err| {
                    tracing::warn!(error = %err, "Failed to initialize Google OAuth provider");
                    err
                })
                .ok()
        });

        Self {
            state_store: OAuthStateStore::new(DEFAULT_STATE_TTL),
            google,
        }
    }
}

fn row_to_user(row: &PgRow) -> Result<UserRecord, sqlx_core::Error> {
    Ok(UserRecord {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        email_verified: row.try_get("email_verified")?,
        display_name: row.try_get("display_name")?,
    })
}

pub async fn upsert_oauth_identity(
    pool: &PgPool,
    provider: &str,
    provider_user_id: &str,
    email: &str,
    email_verified: bool,
    profile: Value,
) -> Result<UserRecord, anyhow::Error> {
    // If identity already exists, update metadata and return the linked user.
    if let Some(row) = query(
        r#"
        SELECT u.id, u.email, u.email_verified, u.display_name
        FROM oauth_identities oi
        JOIN users u ON u.id = oi.user_id
        WHERE oi.provider = $1::oauth_provider AND oi.provider_user_id = $2
        "#,
    )
    .bind(provider)
    .bind(provider_user_id)
    .fetch_optional(pool)
    .await?
    {
        let mut user = row_to_user(&row)?;

        // Update identity metadata
        query(
            r#"
            UPDATE oauth_identities
            SET email = $3, email_verified = $4, profile = $5, updated_at = NOW()
            WHERE provider = $1::oauth_provider AND provider_user_id = $2
            "#,
        )
        .bind(provider)
        .bind(provider_user_id)
        .bind(email)
        .bind(email_verified)
        .bind(profile)
        .execute(pool)
        .await?;

        if email_verified && !user.email_verified {
            query(
                r#"
                UPDATE users
                SET email_verified = TRUE, updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(user.id)
            .execute(pool)
            .await?;
            user.email_verified = true;
        }

        return Ok(user);
    }

    // Optionally link to existing verified email user
    let mut existing_user = if email_verified {
        query(
            r#"
            SELECT id, email, email_verified, display_name
            FROM users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(pool)
        .await?
        .map(|row| row_to_user(&row))
        .transpose()?
    } else {
        None
    };

    // Create a new user if none found
    let user = match existing_user.as_mut() {
        Some(user) => {
            if email_verified && !user.email_verified {
                query(
                    r#"
                    UPDATE users
                    SET email_verified = TRUE, updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(user.id)
                .execute(pool)
                .await?;
                user.email_verified = true;
            }
            user.clone()
        }
        None => {
            let row = query(
                r#"
                INSERT INTO users (email, email_verified, display_name)
                VALUES ($1, $2, NULL)
                RETURNING id, email, email_verified, display_name
                "#,
            )
            .bind(email)
            .bind(email_verified)
            .fetch_one(pool)
            .await?;

            row_to_user(&row)?
        }
    };

    // Upsert the identity
    query(
        r#"
        INSERT INTO oauth_identities (user_id, provider, provider_user_id, email, email_verified, profile)
        VALUES ($1, $2::oauth_provider, $3, $4, $5, $6)
        ON CONFLICT (provider, provider_user_id)
        DO UPDATE
        SET user_id = EXCLUDED.user_id,
            email = EXCLUDED.email,
            email_verified = EXCLUDED.email_verified,
            profile = EXCLUDED.profile,
            updated_at = NOW()
        "#,
    )
    .bind(user.id)
    .bind(provider)
    .bind(provider_user_id)
    .bind(email)
    .bind(email_verified)
    .bind(profile)
    .execute(pool)
    .await?;

    Ok(user)
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionClaims {
    sub: String,
    email: String,
    provider: String,
    exp: usize,
}

pub fn issue_session_token(
    user: &UserRecord,
    provider: &str,
    jwt_secret: &str,
) -> Result<(String, DateTime<Utc>), anyhow::Error> {
    let expires_at = Utc::now()
        + chrono::Duration::from_std(SESSION_TTL)
            .expect("static session ttl should convert to chrono::Duration");

    let claims = SessionClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        provider: provider.to_string(),
        exp: expires_at.timestamp() as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )?;

    Ok((token, expires_at))
}
