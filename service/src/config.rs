use figment::{
    providers::{Env, Format, Serialized, Yaml},
    Figment,
};
use serde::{Deserialize, Serialize};

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
    /// Example: `["http://localhost:5173", "https://app.example.com"]`
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
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
}
