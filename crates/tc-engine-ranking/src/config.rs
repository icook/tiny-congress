//! Engine configuration for ranking rooms.

use chrono::{NaiveTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Defaults ───────────────────────────────────────────────────────────────

const fn default_submit_duration() -> i64 {
    86400
}

const fn default_rank_duration() -> i64 {
    86400
}

const fn default_hall_of_fame_depth() -> i32 {
    3
}

// ─── Config struct ──────────────────────────────────────────────────────────

/// Engine configuration stored in `rooms__rooms.engine_config` for ranking rooms.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RankingConfig {
    /// Daily anchor time in `HH:MM:SS` format.
    pub anchor_time: String,
    /// IANA timezone string (e.g. `"America/New_York"`).
    pub anchor_timezone: String,
    /// How long the submission phase lasts in seconds. Defaults to 86400 (1 day).
    #[serde(default = "default_submit_duration")]
    pub submit_duration_secs: i64,
    /// How long the ranking phase lasts in seconds. Defaults to 86400 (1 day).
    #[serde(default = "default_rank_duration")]
    pub rank_duration_secs: i64,
    /// Number of top submissions to promote to the hall of fame each round.
    /// Defaults to 3.
    #[serde(default = "default_hall_of_fame_depth")]
    pub hall_of_fame_depth: i32,
}

impl RankingConfig {
    /// Compute the number of seconds until the next occurrence of `anchor_time`
    /// in `anchor_timezone`.
    ///
    /// If `anchor_time` is in the future today (in the configured timezone),
    /// returns the delta to that time. If it has already passed, returns the
    /// delta to the same time tomorrow.
    ///
    /// Returns 0 if the timezone or time cannot be parsed (callers validate
    /// these before calling this method).
    #[must_use]
    pub fn seconds_until_next_anchor(&self) -> i64 {
        use chrono::TimeZone as _;

        let Ok(anchor) = NaiveTime::parse_from_str(&self.anchor_time, "%H:%M:%S") else {
            return 0;
        };

        let Ok(tz) = self.anchor_timezone.parse::<chrono_tz::Tz>() else {
            return 0;
        };

        let now_utc = Utc::now();
        let now_local = now_utc.with_timezone(&tz);
        let today = now_local.date_naive();

        // Build candidate datetime for anchor_time today
        let candidate_naive = today.and_time(anchor);
        let candidate = match tz.from_local_datetime(&candidate_naive) {
            chrono::LocalResult::Single(dt) => dt,
            chrono::LocalResult::Ambiguous(earliest, _) => earliest,
            chrono::LocalResult::None => {
                // Clocks skipped this time (DST gap); advance by 1 hour
                let adjusted = candidate_naive + chrono::Duration::hours(1);
                match tz.from_local_datetime(&adjusted) {
                    chrono::LocalResult::Single(dt) => dt,
                    chrono::LocalResult::Ambiguous(earliest, _) => earliest,
                    chrono::LocalResult::None => return 0,
                }
            }
        };

        let delta = candidate.with_timezone(&Utc) - now_utc;
        if delta.num_seconds() > 0 {
            delta.num_seconds()
        } else {
            // Anchor has already passed today; compute delta to tomorrow
            let tomorrow = today + chrono::Duration::days(1);
            let tomorrow_naive = tomorrow.and_time(anchor);
            let tomorrow_candidate = match tz.from_local_datetime(&tomorrow_naive) {
                chrono::LocalResult::Single(dt) => dt,
                chrono::LocalResult::Ambiguous(earliest, _) => earliest,
                chrono::LocalResult::None => return 86400,
            };
            let delta2 = tomorrow_candidate.with_timezone(&Utc) - now_utc;
            delta2.num_seconds().max(0)
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(anchor_time: &str, anchor_timezone: &str) -> RankingConfig {
        RankingConfig {
            anchor_time: anchor_time.to_string(),
            anchor_timezone: anchor_timezone.to_string(),
            submit_duration_secs: default_submit_duration(),
            rank_duration_secs: default_rank_duration(),
            hall_of_fame_depth: default_hall_of_fame_depth(),
        }
    }

    #[test]
    fn seconds_until_anchor_is_positive() {
        // Any valid config should return a non-negative delay.
        let cfg = make_config("12:00:00", "UTC");
        let secs = cfg.seconds_until_next_anchor();
        assert!(secs >= 0, "delay must not be negative");
    }

    #[test]
    fn seconds_until_anchor_is_at_most_one_day() {
        let cfg = make_config("00:00:00", "UTC");
        let secs = cfg.seconds_until_next_anchor();
        // Should be at most 86400 seconds (one full day)
        assert!(secs <= 86400, "delay should be at most one day, got {secs}");
    }

    #[test]
    fn bad_timezone_returns_zero() {
        let cfg = make_config("12:00:00", "Not/A/Timezone");
        assert_eq!(cfg.seconds_until_next_anchor(), 0);
    }

    #[test]
    fn bad_time_returns_zero() {
        let cfg = make_config("25:00:00", "UTC");
        assert_eq!(cfg.seconds_until_next_anchor(), 0);
    }

    #[test]
    fn defaults_applied_on_partial_deserialize() {
        let v = serde_json::json!({
            "anchor_time": "12:00:00",
            "anchor_timezone": "UTC"
        });
        let cfg: RankingConfig = serde_json::from_value(v).unwrap();
        assert_eq!(cfg.submit_duration_secs, 86400);
        assert_eq!(cfg.rank_duration_secs, 86400);
        assert_eq!(cfg.hall_of_fame_depth, 3);
    }
}
