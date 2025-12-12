use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Timelike, Utc};
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum endorsements per account per day
    pub max_per_account_per_day: i32,
    /// Maximum endorsements per subject/topic per account per day
    pub max_per_subject_topic_per_day: i32,
    /// Window duration in hours
    pub window_hours: i64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_per_account_per_day: 50,
            max_per_subject_topic_per_day: 10,
            window_hours: 24,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded for account: {0} endorsements in {1} hours")]
    AccountLimit(i32, i64),
    #[error("Rate limit exceeded for subject {subject_type}:{subject_id} topic {topic}: {count} endorsements in {window_hours} hours")]
    SubjectTopicLimit {
        subject_type: String,
        subject_id: String,
        topic: String,
        count: i32,
        window_hours: i64,
    },
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// Check rate limits for an endorsement
///
/// Returns Ok(()) if within limits, Err(RateLimitError) if exceeded
pub async fn check_rate_limit(
    pool: &PgPool,
    account_id: Uuid,
    subject_type: &str,
    subject_id: &str,
    topic: &str,
    config: &RateLimitConfig,
) -> Result<(), RateLimitError> {
    let window_start = Utc::now() - Duration::hours(config.window_hours);

    // Check account-wide limit
    let account_count = get_account_rate_count(pool, account_id, window_start).await?;
    if account_count >= config.max_per_account_per_day {
        warn!(
            account_id = %account_id,
            count = account_count,
            limit = config.max_per_account_per_day,
            "Account rate limit exceeded"
        );
        return Err(RateLimitError::AccountLimit(
            account_count,
            config.window_hours,
        ));
    }

    // Check subject/topic-specific limit
    let subject_count = get_subject_topic_rate_count(
        pool,
        account_id,
        subject_type,
        subject_id,
        topic,
        window_start,
    )
    .await?;

    if subject_count >= config.max_per_subject_topic_per_day {
        warn!(
            account_id = %account_id,
            subject_type = subject_type,
            subject_id = subject_id,
            topic = topic,
            count = subject_count,
            limit = config.max_per_subject_topic_per_day,
            "Subject/topic rate limit exceeded"
        );
        return Err(RateLimitError::SubjectTopicLimit {
            subject_type: subject_type.to_string(),
            subject_id: subject_id.to_string(),
            topic: topic.to_string(),
            count: subject_count,
            window_hours: config.window_hours,
        });
    }

    Ok(())
}

/// Increment rate limit counter after successful endorsement
pub async fn increment_rate_limit(
    pool: &PgPool,
    account_id: Uuid,
    subject_type: &str,
    subject_id: &str,
    topic: &str,
) -> Result<()> {
    let window_start = get_current_window_start();

    sqlx::query!(
        r#"
        INSERT INTO endorsement_rate_limits (account_id, subject_type, subject_id, topic, window_start, count)
        VALUES ($1, $2, $3, $4, $5, 1)
        ON CONFLICT (account_id, subject_type, subject_id, topic, window_start)
        DO UPDATE SET count = endorsement_rate_limits.count + 1
        "#,
        account_id,
        subject_type,
        subject_id,
        topic,
        window_start
    )
    .execute(pool)
    .await
    .context("Failed to increment rate limit")?;

    Ok(())
}

/// Get account-wide rate count
async fn get_account_rate_count(
    pool: &PgPool,
    account_id: Uuid,
    window_start: DateTime<Utc>,
) -> Result<i32, RateLimitError> {
    let result = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(count), 0)::int as "count!"
        FROM endorsement_rate_limits
        WHERE account_id = $1
          AND window_start >= $2
        "#,
        account_id,
        window_start
    )
    .fetch_one(pool)
    .await?;

    Ok(result)
}

/// Get subject/topic-specific rate count
async fn get_subject_topic_rate_count(
    pool: &PgPool,
    account_id: Uuid,
    subject_type: &str,
    subject_id: &str,
    topic: &str,
    window_start: DateTime<Utc>,
) -> Result<i32, RateLimitError> {
    let result = sqlx::query_scalar!(
        r#"
        SELECT COALESCE(SUM(count), 0)::int as "count!"
        FROM endorsement_rate_limits
        WHERE account_id = $1
          AND subject_type = $2
          AND subject_id = $3
          AND topic = $4
          AND window_start >= $5
        "#,
        account_id,
        subject_type,
        subject_id,
        topic,
        window_start
    )
    .fetch_one(pool)
    .await?;

    Ok(result)
}

/// Get the current window start time (hour boundary)
fn get_current_window_start() -> DateTime<Utc> {
    let now = Utc::now();
    now.date_naive()
        .and_hms_opt(now.hour(), 0, 0)
        .map_or(now, |dt| DateTime::from_naive_utc_and_offset(dt, Utc))
}

/// Clean up old rate limit entries
pub async fn cleanup_old_rate_limits(pool: &PgPool, days_to_keep: i64) -> Result<u64> {
    let cutoff = Utc::now() - Duration::days(days_to_keep);

    let result = sqlx::query!(
        r#"
        DELETE FROM endorsement_rate_limits
        WHERE window_start < $1
        "#,
        cutoff
    )
    .execute(pool)
    .await
    .context("Failed to cleanup old rate limits")?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_per_account_per_day, 50);
        assert_eq!(config.max_per_subject_topic_per_day, 10);
        assert_eq!(config.window_hours, 24);
    }

    #[test]
    fn test_window_start_calculation() {
        let window = get_current_window_start();
        assert_eq!(window.minute(), 0);
        assert_eq!(window.second(), 0);
    }
}
