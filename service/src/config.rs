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
/// 2. config.yaml file (if exists)
/// 3. Environment variables with TC_ prefix (always wins)
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// `PostgreSQL` connection URL (required).
    /// Example: `postgres://user:pass@host:5432/dbname`
    pub url: String,

    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Optional custom migrations directory path.
    pub migrations_dir: Option<String>,
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SwaggerConfig {
    /// Enable Swagger UI at /swagger-ui.
    /// Default: false (disabled for security - exposes API documentation).
    /// Enable in development via `TC_SWAGGER__ENABLED=true`
    #[serde(default)]
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                url: String::new(), // Will fail validation if not provided
                max_connections: default_max_connections(),
                migrations_dir: None,
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
    /// 2. config.yaml file (if exists)
    /// 3. Environment variables with TC_ prefix (highest)
    ///
    /// # Errors
    /// Returns an error if configuration cannot be loaded or is invalid.
    pub fn load() -> Result<Self, ConfigError> {
        let config: Self = Figment::new()
            .merge(Serialized::defaults(Self::default()))
            .merge(Yaml::file("config.yaml"))
            .merge(Env::prefixed("TC_").split("__"))
            .extract()?;

        config.validate()?;
        Ok(config)
    }

    /// Load configuration with a custom YAML file path.
    ///
    /// # Errors
    /// Returns an error if configuration cannot be loaded or is invalid.
    pub fn load_from(yaml_path: &str) -> Result<Self, ConfigError> {
        let config: Self = Figment::new()
            .merge(Serialized::defaults(Self::default()))
            .merge(Yaml::file(yaml_path))
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
        // Database URL is required and must be a postgres URL
        if self.database.url.is_empty() {
            return Err(ConfigError::Validation(
                "database.url is required. Set TC_DATABASE__URL environment variable.".into(),
            ));
        }

        if !self.database.url.starts_with("postgres://")
            && !self.database.url.starts_with("postgresql://")
        {
            return Err(ConfigError::Validation(format!(
                "database.url must start with postgres:// or postgresql://, got: {}",
                &self.database.url[..self.database.url.len().min(20)]
            )));
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let config = Config::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.database.max_connections, 10);
    }

    #[test]
    fn test_validation_rejects_empty_database_url() {
        let config = Config::default();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("database.url is required"));
    }

    #[test]
    fn test_validation_rejects_non_postgres_url() {
        let mut config = Config::default();
        config.database.url = "mysql://localhost/db".into();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with postgres://"));
    }

    #[test]
    fn test_validation_accepts_valid_config() {
        let mut config = Config::default();
        config.database.url = "postgres://localhost/test".into();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_accepts_postgresql_scheme() {
        let mut config = Config::default();
        config.database.url = "postgresql://localhost/test".into();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cors_defaults_to_empty() {
        let config = CorsConfig::default();
        assert!(config.allowed_origins.is_empty());
    }

    #[test]
    fn test_cors_validation_accepts_valid_origins() {
        let mut config = Config::default();
        config.database.url = "postgres://localhost/test".into();
        config.cors.allowed_origins = vec![
            "http://localhost:3000".into(),
            "https://app.example.com".into(),
        ];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cors_validation_accepts_wildcard() {
        let mut config = Config::default();
        config.database.url = "postgres://localhost/test".into();
        config.cors.allowed_origins = vec!["*".into()];
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cors_validation_rejects_invalid_origin() {
        let mut config = Config::default();
        config.database.url = "postgres://localhost/test".into();
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
}
