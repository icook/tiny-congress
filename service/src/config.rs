use std::env;
use tracing::warn;

#[derive(Clone, Debug)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

#[derive(Clone, Debug)]
pub struct JwtConfig {
    pub secret: String,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub google_oauth: Option<GoogleOAuthConfig>,
    pub jwt: Option<JwtConfig>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/tinycongress".to_string()
        });

        let google_client_id = env::var("GOOGLE_CLIENT_ID").ok();
        let google_client_secret = env::var("GOOGLE_CLIENT_SECRET").ok();
        let google_redirect_uri = env::var("GOOGLE_REDIRECT_URI").ok();

        let google_oauth = match (google_client_id, google_client_secret, google_redirect_uri) {
            (Some(client_id), Some(client_secret), Some(redirect_uri)) => Some(GoogleOAuthConfig {
                client_id,
                client_secret,
                redirect_uri,
            }),
            (None, None, None) => None,
            _ => {
                warn!("Partial Google OAuth configuration detected; all GOOGLE_* vars are required to enable login");
                None
            }
        };

        let jwt = env::var("JWT_SECRET")
            .ok()
            .map(|secret| JwtConfig { secret });

        Ok(Self {
            database_url,
            google_oauth,
            jwt,
        })
    }
}
