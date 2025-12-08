mod google;
mod state;

use crate::config::AppConfig;
pub use google::{GoogleOAuthProvider, GoogleUserInfo};
pub use state::OAuthStateStore;
use std::time::Duration;

const DEFAULT_STATE_TTL: Duration = Duration::from_secs(300);

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
