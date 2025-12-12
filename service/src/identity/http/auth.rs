use anyhow::{Context, Result};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

/// JWT claims for session tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: Uuid,       // account_id
    pub device_id: Uuid, // device_id
    pub session_id: Uuid,
    pub iat: i64, // issued at
    pub exp: i64, // expiration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>, // key ID for rotation support
}

/// Sign a session token using HS256
///
/// # Errors
/// Returns an error if `SESSION_SIGNING_KEY` is not set or JWT encoding fails
pub fn sign_session_token(claims: &SessionClaims) -> Result<String> {
    let secret = env::var("SESSION_SIGNING_KEY")
        .context("SESSION_SIGNING_KEY environment variable not set")?;

    let header = Header::new(Algorithm::HS256);
    let token = encode(&header, claims, &EncodingKey::from_secret(secret.as_bytes()))
        .context("Failed to encode JWT")?;

    Ok(token)
}

/// Verify and decode a session token
///
/// # Errors
/// Returns an error if the token is invalid, expired, or `SESSION_SIGNING_KEY` is wrong
pub fn verify_session_token(token: &str) -> Result<SessionClaims> {
    let secret = env::var("SESSION_SIGNING_KEY")
        .context("SESSION_SIGNING_KEY environment variable not set")?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data =
        decode::<SessionClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
            .context("Failed to decode JWT")?;

    Ok(token_data.claims)
}

/// Verify a session token with support for multiple keys (for rotation)
///
/// This tries the current key first, then falls back to an old key if provided.
/// This enables zero-downtime key rotation.
///
/// # Errors
/// Returns an error if the token cannot be verified with any of the provided keys
pub fn verify_session_token_with_rotation(token: &str) -> Result<SessionClaims> {
    // Try current key first
    match verify_session_token(token) {
        Ok(claims) => Ok(claims),
        Err(e) => {
            // If there's an old key, try that
            if let Ok(old_secret) = env::var("SESSION_SIGNING_KEY_OLD") {
                let mut validation = Validation::new(Algorithm::HS256);
                validation.validate_exp = true;

                if let Ok(token_data) = decode::<SessionClaims>(
                    token,
                    &DecodingKey::from_secret(old_secret.as_bytes()),
                    &validation,
                ) {
                    Ok(token_data.claims)
                } else {
                    Err(e)
                }
            } else {
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::env;

    #[test]
    #[allow(clippy::expect_used)]
    fn test_sign_and_verify_token() {
        env::set_var("SESSION_SIGNING_KEY", "test-secret-key-for-testing-only");

        let claims = SessionClaims {
            sub: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            iat: Utc::now().timestamp(),
            exp: (Utc::now() + chrono::Duration::hours(1)).timestamp(),
            kid: None,
        };

        let token = sign_session_token(&claims).expect("Should sign token");
        let verified = verify_session_token(&token).expect("Should verify token");

        assert_eq!(verified.sub, claims.sub);
        assert_eq!(verified.device_id, claims.device_id);
        assert_eq!(verified.session_id, claims.session_id);

        env::remove_var("SESSION_SIGNING_KEY");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_expired_token_fails() {
        env::set_var("SESSION_SIGNING_KEY", "test-secret-key-for-testing-only");

        let claims = SessionClaims {
            sub: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            iat: (Utc::now() - chrono::Duration::hours(2)).timestamp(),
            exp: (Utc::now() - chrono::Duration::hours(1)).timestamp(),
            kid: None,
        };

        let token = sign_session_token(&claims).expect("Should sign token");
        let result = verify_session_token(&token);

        assert!(result.is_err());
        env::remove_var("SESSION_SIGNING_KEY");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_wrong_secret_fails() {
        env::set_var("SESSION_SIGNING_KEY", "secret1");

        let claims = SessionClaims {
            sub: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            iat: Utc::now().timestamp(),
            exp: (Utc::now() + chrono::Duration::hours(1)).timestamp(),
            kid: None,
        };

        let token = sign_session_token(&claims).expect("Should sign token");

        // Change the secret
        env::set_var("SESSION_SIGNING_KEY", "secret2");

        let result = verify_session_token(&token);
        assert!(result.is_err());

        env::remove_var("SESSION_SIGNING_KEY");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_rotation_fallback() {
        env::set_var("SESSION_SIGNING_KEY", "new-secret");
        env::set_var("SESSION_SIGNING_KEY_OLD", "old-secret");

        // Create token with old secret
        env::set_var("SESSION_SIGNING_KEY", "old-secret");
        let claims = SessionClaims {
            sub: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            iat: Utc::now().timestamp(),
            exp: (Utc::now() + chrono::Duration::hours(1)).timestamp(),
            kid: Some("old".to_string()),
        };
        let token = sign_session_token(&claims).expect("Should sign token");

        // Switch to new secret
        env::set_var("SESSION_SIGNING_KEY", "new-secret");

        // Should still verify using the old key
        let verified =
            verify_session_token_with_rotation(&token).expect("Should verify with rotation");
        assert_eq!(verified.sub, claims.sub);

        env::remove_var("SESSION_SIGNING_KEY");
        env::remove_var("SESSION_SIGNING_KEY_OLD");
    }
}
