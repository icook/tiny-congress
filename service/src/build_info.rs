use async_graphql::SimpleObject;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::env;
use utoipa::ToSchema;

/// Build metadata exposed via GraphQL, REST, and logs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, SimpleObject, ToSchema)]
#[graphql(rename_fields = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// Application version (from `APP_VERSION` env var, see #256 for prefix standardization).
    pub version: String,
    /// Git commit SHA.
    pub git_sha: String,
    /// Build timestamp in RFC3339 format.
    pub build_time: String,
    /// Optional build message or description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Provides build metadata from environment variables.
#[derive(Clone, Debug)]
pub struct BuildInfoProvider {
    info: BuildInfo,
}

impl BuildInfoProvider {
    /// Construct a provider using environment variables, falling back to sensible defaults.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Construct a provider using a custom lookup function (useful for tests).
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

        let info = BuildInfo {
            version,
            git_sha,
            build_time,
            message,
        };

        Self { info }
    }

    /// Fetch the resolved build info values.
    #[must_use]
    pub fn build_info(&self) -> BuildInfo {
        self.info.clone()
    }
}

fn normalize_build_time(value: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_rfc3339(&format!("{value}Z")))
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
        .ok()
}
