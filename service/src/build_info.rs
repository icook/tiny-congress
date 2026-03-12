use async_graphql::SimpleObject;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::env;
use utoipa::ToSchema;

/// Build metadata exposed via GraphQL, REST, and logs.
///
/// Loaded from environment variables at startup (see [`BuildInfo::from_env`]).
/// These are typically set by the CI pipeline or Dockerfile at image build time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, SimpleObject, ToSchema)]
#[graphql(rename_fields = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// Application version string. Read from `APP_VERSION` or `VERSION` env var.
    /// Defaults to `"dev"`.
    pub version: String,
    /// Git commit SHA. Read from `GIT_SHA` env var. Defaults to `"unknown"`.
    pub git_sha: String,
    /// Build timestamp in RFC 3339 format. Read from `BUILD_TIME` env var.
    /// Defaults to `"unknown"`.
    pub build_time: String,
    /// Optional build message (e.g., CI run URL). Read from `BUILD_MESSAGE` env var.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl BuildInfo {
    /// Construct build info from environment variables, falling back to sensible defaults.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Construct build info using a custom lookup function (useful for tests).
    #[must_use]
    pub fn from_lookup<F>(mut lookup: F) -> Self
    where
        F: FnMut(&str) -> Option<String>,
    {
        let version = lookup("APP_VERSION")
            .or_else(|| lookup("VERSION"))
            .unwrap_or_else(|| "dev".to_string());

        let git_sha = lookup("GIT_SHA").unwrap_or_else(|| "unknown".to_string());

        let build_time = lookup("BUILD_TIME")
            .and_then(|value| normalize_build_time(&value))
            .unwrap_or_else(|| "unknown".to_string());

        let message = lookup("BUILD_MESSAGE");

        Self {
            version,
            git_sha,
            build_time,
            message,
        }
    }
}

fn normalize_build_time(value: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_rfc3339(&format!("{value}Z")))
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
        .ok()
}
