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
}

impl DatabaseConfig {
    /// Assemble a `PostgreSQL` connection URL from individual fields.
    #[must_use]
    pub fn connection_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
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
                host: default_db_host(),
                port: default_db_port(),
                name: default_db_name(),
                user: String::new(),
                password: String::new(),
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
            return Err(ConfigError::Validation(
                "database.port cannot be 0".into(),
            ));
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

    fn valid_config() -> Config {
        let mut config = Config::default();
        config.database.user = "postgres".into();
        config.database.password = "postgres".into();
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
    }

    #[test]
    fn test_validation_accepts_valid_config() {
        let config = valid_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_database_config_connection_url() {
        let config = DatabaseConfig {
            host: "db.example.com".into(),
            port: 5432,
            name: "mydb".into(),
            user: "admin".into(),
            password: "s3cret".into(),
            max_connections: 10,
            migrations_dir: None,
        };
        assert_eq!(
            config.connection_url(),
            "postgres://admin:s3cret@db.example.com:5432/mydb"
        );
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
}
