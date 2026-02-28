//! In-memory nonce store for replay prevention.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Duration after which nonces are eligible for cleanup (2x max timestamp skew).
const NONCE_TTL: Duration = Duration::from_secs(600);

/// Maximum nonce length to prevent memory abuse.
const MAX_NONCE_LENGTH: usize = 64;

/// In-memory store tracking recently-seen request nonces.
#[derive(Debug)]
pub struct NonceStore {
    seen: RwLock<HashMap<String, Instant>>,
}

impl NonceStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            seen: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a nonce is fresh. Returns `true` if accepted (first use),
    /// `false` if rejected (empty, too long, duplicate, or lock poisoned).
    pub fn check_and_insert(&self, nonce: &str) -> bool {
        if nonce.is_empty() || nonce.len() > MAX_NONCE_LENGTH {
            return false;
        }

        let Ok(mut map) = self.seen.write() else {
            // Lock is poisoned — a previous holder panicked. Reject all nonces
            // rather than silently accepting replays.
            return false;
        };

        // Lazy cleanup when map gets large
        if map.len() > 10_000 {
            if let Some(cutoff) = Instant::now().checked_sub(NONCE_TTL) {
                map.retain(|_, &mut ts| ts > cutoff);
            }
        }

        if map.contains_key(nonce) {
            return false;
        }

        map.insert(nonce.to_string(), Instant::now());
        true
    }
}

impl Default for NonceStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_fresh_nonce() {
        let store = NonceStore::new();
        assert!(store.check_and_insert("nonce-1"));
    }

    #[test]
    fn rejects_duplicate_nonce() {
        let store = NonceStore::new();
        assert!(store.check_and_insert("nonce-dup"));
        assert!(!store.check_and_insert("nonce-dup"));
    }

    #[test]
    fn rejects_empty_nonce() {
        let store = NonceStore::new();
        assert!(!store.check_and_insert(""));
    }

    #[test]
    fn rejects_oversized_nonce() {
        let store = NonceStore::new();
        let long = "x".repeat(MAX_NONCE_LENGTH + 1);
        assert!(!store.check_and_insert(&long));
    }

    #[test]
    fn accepts_max_length_nonce() {
        let store = NonceStore::new();
        let exact = "x".repeat(MAX_NONCE_LENGTH);
        assert!(store.check_and_insert(&exact));
    }
}
