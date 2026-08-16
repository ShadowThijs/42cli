//! Tiny JSON disk cache with per-entry TTL.
//!
//! Used to pre-cache slower endpoints (project graph, cluster occupancy,
//! profiles) so the UI renders instantly on revisit while data stays fresh.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope<T> {
    fetched_at: i64,
    value: T,
}

/// A namespaced cache rooted at `$XDG_CACHE_HOME/42cli`.
#[derive(Debug, Clone)]
pub struct DiskCache {
    root: PathBuf,
}

impl DiskCache {
    pub fn new() -> Self {
        Self {
            root: super::config::cache_dir(),
        }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        // Keys may contain '/' (user logins), which maps nicely onto subdirs.
        self.root.join(format!("{key}.json"))
    }

    /// Fetch a cached value if it exists and is younger than `ttl`.
    pub fn get<T: DeserializeOwned>(&self, key: &str, ttl: Duration) -> Option<T> {
        let bytes = fs::read(self.path_for(key)).ok()?;
        let envelope: CacheEnvelope<T> = serde_json::from_slice(&bytes).ok()?;
        let age = chrono::Utc::now().timestamp() - envelope.fetched_at;
        if age < 0 || Duration::from_secs(age as u64) > ttl {
            return None;
        }
        Some(envelope.value)
    }

    /// Fetch a cached value regardless of age — used to render stale data
    /// instantly while a refresh is in flight.
    pub fn get_stale<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let bytes = fs::read(self.path_for(key)).ok()?;
        let envelope: CacheEnvelope<T> = serde_json::from_slice(&bytes).ok()?;
        Some(envelope.value)
    }

    pub fn put<T: Serialize>(&self, key: &str, value: &T) {
        let envelope = CacheEnvelope {
            fetched_at: chrono::Utc::now().timestamp(),
            value,
        };
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(&envelope) {
            let _ = fs::write(path, bytes);
        }
    }
}

impl Default for DiskCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let cache = DiskCache::new();
        cache.put("test/roundtrip", &vec![1u32, 2, 3]);
        let value: Vec<u32> = cache.get("test/roundtrip", Duration::from_secs(60)).unwrap();
        assert_eq!(value, vec![1, 2, 3]);
    }
}
