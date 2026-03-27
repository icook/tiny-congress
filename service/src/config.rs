use figment::{
    providers::{Env, Format, Serialized, Yaml},
    Figment,
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_aux::prelude::deserialize_vec_from_string_or_vec;

/// Application configuration loaded from multiple sources.
///
/// Configuration is loaded in priority order (lowest to highest):
/// 1. Struct defaults
/// 2. /etc/tc/config.yaml (Kubernetes `ConfigMap` mount, if exists)
/// 3. config.yaml file (if exists, local dev override)
/// 4. Environment variables with TC_ prefix (always wins)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default)]
    pub security_headers: SecurityHeadersConfig,
    #[serde(default)]
    pub graphql: GraphQLConfig,
    #[serde(default)]
    pub swagger: SwaggerConfig,
    /// HMAC key for generating synthetic backup envelopes.
    ///
    /// Required — prevents username enumeration by making `GET /auth/backup/{username}`
    /// return indistinguishable responses for real and non-existent accounts.
    ///
    /// **Must remain stable for the lifetime of a deployment.** Changing it alters
    /// synthetic responses, which can leak which usernames are real.
    ///
    /// Generate with: `openssl rand -base64 48`
    ///
    /// Set via `TC_SYNTHETIC_BACKUP_KEY` environment variable or `synthetic_backup_key`
    /// in config.yaml. In Kubernetes, set via `syntheticBackupKey` in Helm values.
    #[serde(default)]
    pub synthetic_backup_key: String,
    /// ID.me OAuth configuration. Optional — verification is disabled when absent.
    #[serde(default)]
    pub idme: Option<IdMeConfig>,
    /// Platform verifiers bootstrapped at startup. Each entry creates an account
    /// (if missing) and grants the `authorized_verifier` endorsement.
    /// Set via `TC_VERIFIERS` as a JSON array.
    #[serde(default)]
    pub verifiers: Vec<VerifierConfig>,
    /// Rate limiting for unauthenticated auth endpoints.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

/// Configuration for a platform-bootstrapped verifier account.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerifierConfig {
    /// Username for the verifier account.
    pub name: String,
    /// Base64url-encoded Ed25519 public key (root key for the verifier account).
    pub public_key: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// Database host.
    #[serde(default = "default_db_host")]
    pub host: String,

    /// Database port.
    #[serde(default = "default_db_port")]
    pub port: u16,

    /// Database name.
    #[serde(default = "default_db_name")]
    pub name: String,

    /// Database user (required — no compiled-in default).
    #[serde(default)]
    pub user: String,

    /// Database password (required — no compiled-in default).
    #[serde(default)]
    pub password: String,

    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Optional custom migrations directory path.
    pub migrations_dir: Option<String>,

    /// When true, automatically drop and recreate the database if migrations
    /// fail due to version mismatch (e.g. a different image was deployed
    /// against an existing DB). Only safe for ephemeral databases like the
    /// demo environment. Default: false.
    #[serde(default)]
    pub auto_reset_on_migration_failure: bool,
}

impl std::fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("name", &self.name)
            .field("user", &self.user)
            .field("password", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("migrations_dir", &self.migrations_dir)
            .field(
                "auto_reset_on_migration_failure",
                &self.auto_reset_on_migration_failure,
            )
            .finish()
    }
}

impl DatabaseConfig {
    /// Build `PgConnectOptions` from individual fields.
    ///
    /// Uses structured options instead of URL assembly to avoid issues with
    /// URL-reserved characters in usernames or passwords.
    #[must_use]
    pub fn connect_options(&self) -> sqlx_postgres::PgConnectOptions {
        sqlx_postgres::PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(&self.name)
            .username(&self.user)
            .password(&self.password)
    }

    /// Build `PgConnectOptions` targeting the `postgres` system database.
    ///
    /// Used for administrative operations (DROP/CREATE DATABASE) that cannot
    /// run against the application database itself.
    #[must_use]
    pub fn system_connect_options(&self) -> sqlx_postgres::PgConnectOptions {
        sqlx_postgres::PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database("postgres")
            .username(&self.user)
            .password(&self.password)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// HTTP server port.
    #[serde(default = "default_port")]
    pub port: u16,

    /// HTTP server bind address.
    #[serde(default = "default_host")]
    pub host: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// Log level filter (debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub level: String,
}

/// CORS configuration.
///
/// **Security:** Defaults to blocking all cross-origin requests (empty origin list).
/// You must explicitly configure allowed origins for the frontend to work.
///
/// Set via `TC_CORS__ALLOWED_ORIGINS` (comma-separated) or `cors.allowed_origins`
/// in config.yaml.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorsConfig {
    /// Allowed origins for CORS requests.
    /// Use `"*"` to allow any origin (not recommended for production).
    /// Accepts either an array or comma-separated string.
    /// Example: `["http://localhost:5173"]` or `"http://localhost:5173,https://app.example.com"`
    #[serde(
        default = "default_allowed_origins",
        deserialize_with = "deserialize_origins"
    )]
    pub allowed_origins: Vec<String>,
}

/// Deserialize origins from comma-separated string or array, filtering empty values.
fn deserialize_origins<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let origins: Vec<String> = deserialize_vec_from_string_or_vec(deserializer)?;
    Ok(origins.into_iter().filter(|s| !s.is_empty()).collect())
}

// These functions cannot be const because serde uses function pointers for defaults
#[allow(clippy::missing_const_for_fn)]
fn default_max_connections() -> u32 {
    10
}

#[allow(clippy::missing_const_for_fn)]
fn default_port() -> u16 {
    8080
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_db_host() -> String {
    "localhost".to_string()
}

#[allow(clippy::missing_const_for_fn)]
fn default_db_port() -> u16 {
    5432
}

fn default_db_name() -> String {
    "tiny-congress".to_string()
}

#[allow(clippy::missing_const_for_fn)]
fn default_allowed_origins() -> Vec<String> {
    // Default to empty (no cross-origin requests allowed) - safe for production
    // Configure explicitly via TC_CORS__ALLOWED_ORIGINS or config.yaml
    vec![]
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_allowed_origins(),
        }
    }
}

/// Security response headers configuration.
///
/// When enabled (the default), the following headers are always applied:
/// - `X-Content-Type-Options: nosniff`
/// - `X-XSS-Protection: 1; mode=block`
///
/// The remaining headers below are configurable.
///
/// Set via `TC_SECURITY_HEADERS__*` environment variables or `security_headers.*`
/// in config.yaml.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityHeadersConfig {
    /// Enable security headers (default: true).
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Enable HSTS header (default: false, enable in production with HTTPS).
    #[serde(default)]
    pub hsts_enabled: bool,

    /// HSTS max-age in seconds (default: 31536000 = 1 year).
    #[serde(default = "default_hsts_max_age")]
    pub hsts_max_age: u64,

    /// Include subdomains in HSTS (default: true).
    #[serde(default = "default_true")]
    pub hsts_include_subdomains: bool,

    /// X-Frame-Options value: "DENY" or "SAMEORIGIN" (default: "DENY").
    #[serde(default = "default_frame_options")]
    pub frame_options: String,

    /// Content-Security-Policy header value (default: "default-src 'self'").
    #[serde(default = "default_csp")]
    pub content_security_policy: String,

    /// Referrer-Policy header value (default: "strict-origin-when-cross-origin").
    #[serde(default = "default_referrer_policy")]
    pub referrer_policy: String,

    /// Permissions-Policy header value (default: restrict camera, microphone, geolocation).
    #[serde(default = "default_permissions_policy")]
    pub permissions_policy: String,
}

#[allow(clippy::missing_const_for_fn)]
fn default_true() -> bool {
    true
}

#[allow(clippy::missing_const_for_fn)]
fn default_hsts_max_age() -> u64 {
    31_536_000 // 1 year
}

fn default_frame_options() -> String {
    "DENY".to_string()
}

fn default_csp() -> String {
    "default-src 'self'".to_string()
}

fn default_referrer_policy() -> String {
    "strict-origin-when-cross-origin".to_string()
}

fn default_permissions_policy() -> String {
    "camera=(), microphone=(), geolocation=()".to_string()
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            hsts_enabled: false,
            hsts_max_age: default_hsts_max_age(),
            hsts_include_subdomains: default_true(),
            frame_options: default_frame_options(),
            content_security_policy: default_csp(),
            referrer_policy: default_referrer_policy(),
            permissions_policy: default_permissions_policy(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GraphQLConfig {
    /// Enable GraphQL Playground UI at /graphql (GET).
    /// Default: false (disabled for security - exposes schema to potential attackers).
    /// Enable in development via `TC_GRAPHQL__PLAYGROUND_ENABLED=true`
    #[serde(default)]
    pub playground_enabled: bool,
}

/// ID.me OAuth 2.0 configuration.
///
/// The entire section is optional — if omitted, identity verification is disabled.
/// But if any `TC_IDME__*` variable is set, all required fields must be present.
///
/// Set via `TC_IDME__*` environment variables or `idme.*` in config.yaml.
#[derive(Clone, Deserialize, Serialize)]
pub struct IdMeConfig {
    /// OAuth client ID from ID.me application registration.
    pub client_id: String,
    /// OAuth client secret from ID.me application registration.
    pub client_secret: String,
    #[serde(default = "default_idme_authorize_url")]
    pub authorize_url: String,
    #[serde(default = "default_idme_token_url")]
    pub token_url: String,
    #[serde(default = "default_idme_userinfo_url")]
    pub userinfo_url: String,
    /// The callback URL that ID.me redirects to after authorization.
    /// Must match the redirect URI registered with ID.me.
    pub redirect_uri: String,
    /// HMAC key for signing OAuth state parameters (anti-CSRF).
    /// Must be at least 32 bytes.
    pub state_secret: String,
    /// Frontend URL to redirect to after callback processing.
    /// The result (success/error) is appended as query parameters.
    pub frontend_callback_url: String,
}

impl std::fmt::Debug for IdMeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdMeConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("userinfo_url", &self.userinfo_url)
            .field("redirect_uri", &self.redirect_uri)
            .field("state_secret", &"[REDACTED]")
            .field("frontend_callback_url", &self.frontend_callback_url)
            .finish()
    }
}

fn default_idme_authorize_url() -> String {
    "https://api.idmelabs.com/oauth/authorize".to_string()
}

fn default_idme_token_url() -> String {
    "https://api.idmelabs.com/oauth/token".to_string()
}

fn default_idme_userinfo_url() -> String {
    "https://api.idmelabs.com/api/public/v3/userinfo".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SwaggerConfig {
    /// Enable Swagger UI at /swagger-ui.
    /// Default: false (disabled for security - exposes API documentation).
    /// Enable in development via `TC_SWAGGER__ENABLED=true`
    #[serde(default)]
    pub enabled: bool,
}

/// Rate limiting configuration for unauthenticated auth endpoints.
///
/// Set via `TC_RATE_LIMIT__*` environment variables or `rate_limit.*` in config.yaml.
///
/// Rate limiting is **enabled by default** with conservative limits.
/// Set `TC_RATE_LIMIT__ENABLED=false` to disable (for tests or local dev).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    /// Max signup requests per minute per IP (default: 5).
    #[serde(default = "default_signup_per_minute")]
    pub signup_per_minute: u32,

    /// Max login requests per minute per IP (default: 10).
    #[serde(default = "default_login_per_minute")]
    pub login_per_minute: u32,

    /// Max backup retrieval requests per minute per IP (default: 10).
    #[serde(default = "default_backup_per_minute")]
    pub backup_per_minute: u32,

    /// Enable rate limiting (default: true). Set to false in tests.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[allow(clippy::missing_const_for_fn)]
fn default_signup_per_minute() -> u32 {
    5
}

#[allow(clippy::missing_const_for_fn)]
fn default_login_per_minute() -> u32 {
    10
}

#[allow(clippy::missing_const_for_fn)]
fn default_backup_per_minute() -> u32 {
    10
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            signup_per_minute: default_signup_per_minute(),
            login_per_minute: default_login_per_minute(),
            backup_per_minute: default_backup_per_minute(),
            enabled: default_true(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                host: default_db_host(),
                port: default_db_port(),
                name: default_db_name(),
                user: String::new(),
                password: String::new(),
                max_connections: default_max_connections(),
                migrations_dir: None,
                auto_reset_on_migration_failure: false,
            },
            server: ServerConfig {
                port: default_port(),
                host: default_host(),
            },
            logging: LoggingConfig {
                level: default_log_level(),
            },
            cors: CorsConfig::default(),
            security_headers: SecurityHeadersConfig::default(),
            graphql: GraphQLConfig::default(),
            swagger: SwaggerConfig::default(),
            synthetic_backup_key: String::new(),
            idme: None,
            verifiers: Vec::new(),
            rate_limit: RateLimitConfig::default(),
        }
    }
}

/// Configuration loading and validation errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration error: {0}")]
    Figment(#[from] Box<figment::Error>),

    #[error("Validation error: {0}")]
    Validation(String),
}

impl From<figment::Error> for ConfigError {
    fn from(err: figment::Error) -> Self {
        Self::Figment(Box::new(err))
    }
}

impl Config {
    /// Load configuration from all sources.
    ///
    /// Sources are merged in priority order:
    /// 1. Struct defaults (lowest)
    /// 2. /etc/tc/config.yaml (Kubernetes `ConfigMap` mount, if exists)
    /// 3. config.yaml file (if exists, local dev override)
    /// 4. Environment variables with TC_ prefix (highest)
    ///
    /// # Errors
    /// Returns an error if configuration cannot be loaded or is invalid.
    pub fn load() -> Result<Self, ConfigError> {
        let config: Self = Figment::new()
            .merge(Serialized::defaults(Self::default()))
            .merge(Yaml::file("/etc/tc/config.yaml"))
            .merge(Yaml::file("config.yaml"))
            .merge(Env::prefixed("TC_").split("__"))
            .extract()?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values.
    ///
    /// # Errors
    /// Returns an error if any configuration value is invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Database user is required
        if self.database.user.is_empty() {
            return Err(ConfigError::Validation(
                "database.user is required. Set TC_DATABASE__USER environment variable or configure in config.yaml.".into(),
            ));
        }

        // Database password is required
        if self.database.password.is_empty() {
            return Err(ConfigError::Validation(
                "database.password is required. Set TC_DATABASE__PASSWORD environment variable or configure in config.yaml.".into(),
            ));
        }

        // Database port must be non-zero
        if self.database.port == 0 {
            return Err(ConfigError::Validation("database.port cannot be 0".into()));
        }

        // Port must be non-zero
        if self.server.port == 0 {
            return Err(ConfigError::Validation("server.port cannot be 0".into()));
        }

        // Max connections must be at least 1
        if self.database.max_connections == 0 {
            return Err(ConfigError::Validation(
                "database.max_connections cannot be 0".into(),
            ));
        }

        // CORS origins must be valid URLs or "*"
        for origin in &self.cors.allowed_origins {
            if origin != "*" && !origin.starts_with("http://") && !origin.starts_with("https://") {
                return Err(ConfigError::Validation(format!(
                    "cors.allowed_origins contains invalid origin '{origin}'. Must be '*' or start with http:// or https://"
                )));
            }
        }

        // X-Frame-Options must be DENY or SAMEORIGIN
        let frame_opts = self.security_headers.frame_options.to_uppercase();
        if frame_opts != "DENY" && frame_opts != "SAMEORIGIN" {
            return Err(ConfigError::Validation(format!(
                "security_headers.frame_options must be 'DENY' or 'SAMEORIGIN', got: '{}'",
                self.security_headers.frame_options
            )));
        }

        // Synthetic backup HMAC key is required and must be at least 32 bytes
        if self.synthetic_backup_key.is_empty() {
            return Err(ConfigError::Validation(
                "synthetic_backup_key is required. Set TC_SYNTHETIC_BACKUP_KEY environment variable or configure in config.yaml.".into(),
            ));
        }
        if self.synthetic_backup_key.len() < 32 {
            return Err(ConfigError::Validation(
                "TC_SYNTHETIC_BACKUP_KEY must be at least 32 bytes".into(),
            ));
        }

        // If IdMe is configured, validate all required fields
        if let Some(ref idme) = self.idme {
            if idme.client_id.is_empty() {
                return Err(ConfigError::Validation(
                    "idme.client_id is required when IdMe is enabled. Set TC_IDME__CLIENT_ID."
                        .into(),
                ));
            }
            if idme.client_secret.is_empty() {
                return Err(ConfigError::Validation(
                    "idme.client_secret is required when IdMe is enabled. Set TC_IDME__CLIENT_SECRET.".into(),
                ));
            }
            if idme.redirect_uri.is_empty() {
                return Err(ConfigError::Validation(
                    "idme.redirect_uri is required when IdMe is enabled. Set TC_IDME__REDIRECT_URI.".into(),
                ));
            }
            if idme.frontend_callback_url.is_empty() {
                return Err(ConfigError::Validation(
                    "idme.frontend_callback_url is required when IdMe is enabled. Set TC_IDME__FRONTEND_CALLBACK_URL.".into(),
                ));
            }
            if idme.state_secret.len() < 32 {
                return Err(ConfigError::Validation(
                    "idme.state_secret must be at least 32 bytes. Set TC_IDME__STATE_SECRET."
                        .into(),
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        let mut config = Config::default();
        config.database.user = "postgres".into();
        config.database.password = "postgres".into();
        config.synthetic_backup_key = "test-hmac-key-for-unit-tests-32+b".into();
        config
    }

    #[test]
    fn test_defaults() {
        let config = Config::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.database.max_connections, 10);
        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.port, 5432);
        assert_eq!(config.database.name, "tiny-congress");
        assert!(config.database.user.is_empty());
        assert!(config.database.password.is_empty());
        assert_eq!(config.rate_limit.signup_per_minute, 5);
        assert_eq!(config.rate_limit.login_per_minute, 10);
        assert_eq!(config.rate_limit.backup_per_minute, 10);
        assert!(config.rate_limit.enabled);
    }

    #[test]
    fn test_validation_accepts_valid_config() {
        let config = valid_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_database_config_connect_options() {
        let config = DatabaseConfig {
            host: "db.example.com".into(),
            port: 5432,
            name: "mydb".into(),
            user: "admin".into(),
            password: "s3cret".into(),
            max_connections: 10,
            migrations_dir: None,
            auto_reset_on_migration_failure: false,
        };
        let opts = config.connect_options();
        // PgConnectOptions exposes getters for host, port, and database
        assert_eq!(opts.get_host(), "db.example.com");
        assert_eq!(opts.get_port(), 5432);
        assert_eq!(opts.get_database().unwrap(), "mydb");
    }

    #[test]
    fn test_database_config_connect_options_special_chars() {
        // Passwords with URL-reserved characters must not break the connection
        let config = DatabaseConfig {
            host: "localhost".into(),
            port: 5432,
            name: "testdb".into(),
            user: "user@org".into(),
            password: "p@ss:word/ok?".into(),
            max_connections: 10,
            migrations_dir: None,
            auto_reset_on_migration_failure: false,
        };
        let opts = config.connect_options();
        // PgConnectOptions handles special chars without URL encoding issues.
        // Note: there are no public getters for username/password (by design), so
        // this only smoke-tests that construction succeeds; end-to-end credential
        // verification requires an integration test against a live Postgres server.
        assert_eq!(opts.get_host(), "localhost");
        assert_eq!(opts.get_database().unwrap(), "testdb");
    }

    #[test]
    fn test_validation_rejects_empty_database_user() {
        let mut config = valid_config();
        config.database.user = "".into();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("database.user"));
    }

    #[test]
    fn test_validation_rejects_empty_database_password() {
        let mut config = valid_config();
        config.database.password = "".into();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("database.password"));
    }

    #[test]
    fn test_validation_rejects_zero_database_port() {
        let mut config = valid_config();
        config.database.port = 0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("database.port"));
    }

    #[test]
    fn test_cors_defaults_to_empty() {
        let config = CorsConfig::default();
        assert!(config.allowed_origins.is_empty());
    }

    #[test]
    fn test_cors_validation_accepts_valid_origins() {
        let mut config = valid_config();
        config.cors.allowed_origins = vec![
            "http://localhost:3000".into(),
            "https://app.example.com".into(),
        ];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cors_validation_accepts_wildcard() {
        let mut config = valid_config();
        config.cors.allowed_origins = vec!["*".into()];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cors_validation_rejects_invalid_origin() {
        let mut config = valid_config();
        config.cors.allowed_origins = vec!["not-a-url".into()];
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid origin"));
    }

    #[test]
    fn test_cors_deserialize_comma_separated_string() {
        // Simulate what figment does with env var
        let json = r#"{"allowed_origins": "http://localhost:5173,https://app.example.com"}"#;
        let config: CorsConfig = serde_json::from_str(json).expect("should parse");
        assert_eq!(config.allowed_origins.len(), 2);
        assert_eq!(config.allowed_origins[0], "http://localhost:5173");
        assert_eq!(config.allowed_origins[1], "https://app.example.com");
    }

    #[test]
    fn test_cors_deserialize_array() {
        let json = r#"{"allowed_origins": ["http://localhost:5173", "https://app.example.com"]}"#;
        let config: CorsConfig = serde_json::from_str(json).expect("should parse");
        assert_eq!(config.allowed_origins.len(), 2);
        assert_eq!(config.allowed_origins[0], "http://localhost:5173");
        assert_eq!(config.allowed_origins[1], "https://app.example.com");
    }

    #[test]
    fn test_cors_deserialize_empty_string() {
        let json = r#"{"allowed_origins": ""}"#;
        let config: CorsConfig = serde_json::from_str(json).expect("should parse");
        assert!(config.allowed_origins.is_empty());
    }

    #[test]
    fn test_graphql_playground_disabled_by_default() {
        let config = GraphQLConfig::default();
        assert!(!config.playground_enabled);
    }

    #[test]
    fn test_graphql_playground_can_be_enabled() {
        let json = r#"{"playground_enabled": true}"#;
        let config: GraphQLConfig = serde_json::from_str(json).expect("should parse");
        assert!(config.playground_enabled);
    }

    #[test]
    fn test_swagger_disabled_by_default() {
        let config = SwaggerConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_swagger_can_be_enabled() {
        let json = r#"{"enabled": true}"#;
        let config: SwaggerConfig = serde_json::from_str(json).expect("should parse");
        assert!(config.enabled);
    }

    // Table-driven boundary tests for validation rules

    #[test]
    fn port_boundaries() {
        let cases = [
            (0u16, false, "zero port"),
            (1, true, "minimum valid port"),
            (80, true, "common HTTP port"),
            (8080, true, "default port"),
            (65535, true, "maximum port"),
        ];

        for (port, should_pass, desc) in cases {
            let mut config = valid_config();
            config.server.port = port;
            let result = config.validate();
            assert_eq!(result.is_ok(), should_pass, "case '{}': {:?}", desc, result);
        }
    }

    #[test]
    fn max_connections_boundaries() {
        let cases = [
            (0u32, false, "zero connections"),
            (1, true, "minimum valid"),
            (10, true, "default value"),
            (100, true, "high value"),
        ];

        for (max, should_pass, desc) in cases {
            let mut config = valid_config();
            config.database.max_connections = max;
            let result = config.validate();
            assert_eq!(result.is_ok(), should_pass, "case '{}': {:?}", desc, result);
        }
    }

    #[test]
    fn cors_origin_boundaries() {
        let cases = [
            (vec!["*"], true, "wildcard"),
            (vec!["http://localhost"], true, "http localhost"),
            (vec!["https://example.com"], true, "https domain"),
            (vec!["http://localhost:3000"], true, "with port"),
            (vec![], true, "empty list"),
            (vec!["ftp://files.com"], false, "ftp scheme"),
            (vec!["localhost"], false, "no scheme"),
            (vec!["//example.com"], false, "protocol-relative"),
        ];

        for (origins, should_pass, desc) in cases {
            let mut config = valid_config();
            config.cors.allowed_origins = origins.into_iter().map(String::from).collect();
            let result = config.validate();
            assert_eq!(result.is_ok(), should_pass, "case '{}': {:?}", desc, result);
        }
    }

    #[test]
    fn frame_options_boundaries() {
        let cases = [
            ("DENY", true, "uppercase DENY"),
            ("SAMEORIGIN", true, "uppercase SAMEORIGIN"),
            ("deny", true, "lowercase deny"),
            ("sameorigin", true, "lowercase sameorigin"),
            ("Deny", true, "mixed case Deny"),
            ("ALLOW-FROM", false, "deprecated ALLOW-FROM"),
            ("", false, "empty string"),
            ("INVALID", false, "invalid value"),
        ];

        for (value, should_pass, desc) in cases {
            let mut config = valid_config();
            config.security_headers.frame_options = value.into();
            let result = config.validate();
            assert_eq!(result.is_ok(), should_pass, "case '{}': {:?}", desc, result);
        }
    }

    fn valid_idme_config() -> IdMeConfig {
        IdMeConfig {
            client_id: "client123".into(),
            client_secret: "secret456".into(),
            authorize_url: default_idme_authorize_url(),
            token_url: default_idme_token_url(),
            userinfo_url: default_idme_userinfo_url(),
            redirect_uri: "https://example.com/callback".into(),
            state_secret: "a-state-secret-that-is-at-least-32-bytes!!".into(),
            frontend_callback_url: "https://example.com/verify".into(),
        }
    }

    #[test]
    fn test_idme_absent_passes_validation() {
        let config = valid_config();
        assert!(config.idme.is_none());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_idme_fully_valid_passes_validation() {
        let mut config = valid_config();
        config.idme = Some(valid_idme_config());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn idme_field_boundaries() {
        // Each case: (field description, how to corrupt it, expected error substring)
        let cases: &[(&str, fn(&mut IdMeConfig), &str)] = &[
            (
                "empty client_id",
                |c| c.client_id = "".into(),
                "idme.client_id",
            ),
            (
                "empty client_secret",
                |c| c.client_secret = "".into(),
                "idme.client_secret",
            ),
            (
                "empty redirect_uri",
                |c| c.redirect_uri = "".into(),
                "idme.redirect_uri",
            ),
            (
                "empty frontend_callback_url",
                |c| c.frontend_callback_url = "".into(),
                "idme.frontend_callback_url",
            ),
            (
                "state_secret too short (31 bytes)",
                |c| c.state_secret = "a".repeat(31),
                "idme.state_secret",
            ),
        ];

        for (desc, corrupt, expected_msg) in cases {
            let mut config = valid_config();
            let mut idme = valid_idme_config();
            corrupt(&mut idme);
            config.idme = Some(idme);
            let result = config.validate();
            assert!(result.is_err(), "case '{}' should fail validation", desc);
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains(expected_msg),
                "case '{}': expected error containing '{}', got: {}",
                desc,
                expected_msg,
                err
            );
        }
    }

    #[test]
    fn test_idme_state_secret_exactly_32_bytes_passes() {
        let mut config = valid_config();
        let mut idme = valid_idme_config();
        idme.state_secret = "a".repeat(32);
        config.idme = Some(idme);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn database_config_debug_redacts_password() {
        let config = DatabaseConfig {
            host: "localhost".into(),
            port: 5432,
            name: "testdb".into(),
            user: "admin".into(),
            password: "super-secret".into(),
            max_connections: 10,
            migrations_dir: None,
            auto_reset_on_migration_failure: false,
        };
        let debug = format!("{config:?}");
        assert!(
            !debug.contains("super-secret"),
            "password must not appear in Debug output"
        );
        assert!(
            debug.contains("[REDACTED]"),
            "password field must show [REDACTED]"
        );
        assert!(
            debug.contains("admin"),
            "non-secret fields must still appear"
        );
    }

    #[test]
    fn idme_config_debug_redacts_client_secret_and_state_secret() {
        let config = valid_idme_config();
        let debug = format!("{config:?}");
        assert!(
            !debug.contains("secret456"),
            "client_secret must not appear in Debug output"
        );
        assert!(
            !debug.contains("a-state-secret"),
            "state_secret must not appear in Debug output"
        );
        assert!(
            debug.matches("[REDACTED]").count() == 2,
            "exactly two fields must be redacted"
        );
        assert!(
            debug.contains("client123"),
            "non-secret client_id must still appear"
        );
    }
}
