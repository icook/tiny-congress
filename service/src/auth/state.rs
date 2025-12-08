use oauth2::PkceCodeVerifier;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct OAuthStateStore {
    ttl: Duration,
    entries: Arc<RwLock<HashMap<String, StateEntry>>>,
}

struct StateEntry {
    pkce_verifier: PkceCodeVerifier,
    created_at: Instant,
}

impl OAuthStateStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Persist a PKCE verifier for the given state value.
    pub async fn put(&self, state: &str, pkce_verifier: PkceCodeVerifier) {
        let mut entries = self.entries.write().await;
        Self::prune_expired(self.ttl, &mut entries);
        entries.insert(
            state.to_string(),
            StateEntry {
                pkce_verifier,
                created_at: Instant::now(),
            },
        );
    }

    /// Fetch and remove the PKCE verifier for this state if it is present and unexpired.
    pub async fn take(&self, state: &str) -> Option<PkceCodeVerifier> {
        let mut entries = self.entries.write().await;
        Self::prune_expired(self.ttl, &mut entries);
        entries.remove(state).map(|entry| entry.pkce_verifier)
    }

    fn prune_expired(ttl: Duration, entries: &mut HashMap<String, StateEntry>) {
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.created_at) <= ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_and_retrieves_pkce_verifier() {
        let store = OAuthStateStore::new(Duration::from_secs(60));
        let verifier = PkceCodeVerifier::new("test-verifier".to_string());
        let verifier_secret = verifier.secret().to_owned();

        store.put("state-1", verifier).await;
        let retrieved = store.take("state-1").await;

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().secret(), &verifier_secret);

        // State should be removed after retrieval
        assert!(store.take("state-1").await.is_none());
    }

    #[tokio::test]
    async fn drops_expired_states() {
        let store = OAuthStateStore::new(Duration::from_millis(10));
        let verifier = PkceCodeVerifier::new("short-lived".to_string());

        store.put("state-2", verifier).await;
        tokio::time::sleep(Duration::from_millis(30)).await;

        assert!(store.take("state-2").await.is_none());
    }
}
