use crate::config::GoogleOAuthConfig;
use oauth2::{
    basic::BasicClient, reqwest::async_http_client, url::Url, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenUrl,
};
use serde::Deserialize;
use std::time::Duration;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
const EMAIL_SCOPE: &str = "https://www.googleapis.com/auth/userinfo.email";

#[derive(Clone)]
pub struct GoogleOAuthProvider {
    client: BasicClient,
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub email: String,
    #[serde(default)]
    pub email_verified: bool,
}

impl GoogleOAuthProvider {
    pub fn new(config: &GoogleOAuthConfig) -> Result<Self, anyhow::Error> {
        let client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            Some(ClientSecret::new(config.client_secret.clone())),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string())?,
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string())?),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_uri.clone())?);

        Ok(Self { client })
    }

    pub fn authorization_url(&self) -> (Url, CsrfToken, PkceCodeVerifier) {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let mut request = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(EMAIL_SCOPE.to_string()))
            .set_pkce_challenge(pkce_challenge);

        // Request offline access to allow refresh tokens if we later choose to store them.
        request = request.add_extra_param("access_type", "offline");

        let (auth_url, csrf) = request.url();
        (auth_url, csrf, pkce_verifier)
    }

    pub async fn exchange_code(
        &self,
        code: AuthorizationCode,
        pkce_verifier: PkceCodeVerifier,
    ) -> Result<oauth2::basic::BasicTokenResponse, anyhow::Error> {
        let token_response = self
            .client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(async_http_client)
            .await?;

        Ok(token_response)
    }

    pub async fn fetch_user_info(
        &self,
        access_token: &oauth2::AccessToken,
    ) -> Result<GoogleUserInfo, anyhow::Error> {
        let client = reqwest::Client::new();
        let response = client
            .get(GOOGLE_USERINFO_URL)
            .bearer_auth(access_token.secret())
            .timeout(Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?;

        let payload = response.json::<GoogleUserInfo>().await?;
        Ok(payload)
    }
}
