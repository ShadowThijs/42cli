//! Fast binary disk cache with per-entry TTL + in-memory layer.
//!
//! Primary format is bincode (`*.bin`) for minimal deserialization overhead.
//! Legacy JSON (`*.json`) is read as fallback and transparently upgraded.
//! An in-memory HashMap avoids repeated disk I/O within one run.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct CacheEnvelope<T> {
    fetched_at: i64,
    value: T,
}

type CacheEntry = (i64, Vec<u8>);
type CacheMap = HashMap<String, CacheEntry>;

/// A namespaced cache rooted at `$XDG_CACHE_HOME/42cli`.
#[derive(Debug, Clone)]
pub struct DiskCache {
    root: PathBuf,
    mem: Arc<RwLock<CacheMap>>,
}

impl DiskCache {
    pub fn new() -> Self {
        Self {
            root: super::config::cache_dir(),
            mem: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn path_bin(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.bin"))
    }
    fn path_json(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.json"))
    }

    fn read_envelope<T: DeserializeOwned + Serialize>(
        &self,
        key: &str,
    ) -> Option<CacheEnvelope<T>> {
        // 1. In-memory fast path.
        if let Some((fetched_at, bytes)) = self.mem.read().ok().and_then(|m| m.get(key).cloned())
            && let Ok(val) = bincode::deserialize::<T>(&bytes)
        {
            return Some(CacheEnvelope {
                fetched_at,
                value: val,
            });
        }
        // 2. Disk: try bin first.
        if let Ok(bytes) = fs::read(self.path_bin(key))
            && let Ok(env) = bincode::deserialize::<CacheEnvelope<T>>(&bytes)
        {
            // populate mem
            if let Ok(val_bytes) = bincode::serialize(&env.value) {
                let _ = self
                    .mem
                    .write()
                    .map(|mut m| m.insert(key.to_owned(), (env.fetched_at, val_bytes)));
            }
            return Some(env);
        }
        // 3. Legacy JSON fallback — also migrates to bin on next put, but we
        //    return the value now for optimistic display.
        if let Ok(bytes) = fs::read(self.path_json(key))
            && let Ok(env) = serde_json::from_slice::<CacheEnvelope<T>>(&bytes)
        {
            if let Ok(val_bytes) = bincode::serialize(&env.value) {
                let _ = self
                    .mem
                    .write()
                    .map(|mut m| m.insert(key.to_owned(), (env.fetched_at, val_bytes)));
            }
            return Some(env);
        }
        None
    }

    /// Fetch a cached value if it exists and is younger than `ttl`.
    pub fn get<T: DeserializeOwned + Serialize>(&self, key: &str, ttl: Duration) -> Option<T> {
        let env = self.read_envelope::<T>(key)?;
        let age = chrono::Utc::now().timestamp() - env.fetched_at;
        if age < 0 || Duration::from_secs(age as u64) > ttl {
            return None;
        }
        Some(env.value)
    }

    /// Fetch regardless of age.
    pub fn get_stale<T: DeserializeOwned + Serialize>(&self, key: &str) -> Option<T> {
        self.read_envelope::<T>(key).map(|e| e.value)
    }

    /// Like `get` but also returns age for stale-while-revalidate decisions.
    pub fn get_with_age<T: DeserializeOwned + Serialize>(
        &self,
        key: &str,
    ) -> Option<(T, Duration)> {
        let env = self.read_envelope::<T>(key)?;
        let age_secs = (chrono::Utc::now().timestamp() - env.fetched_at).max(0) as u64;
        Some((env.value, Duration::from_secs(age_secs)))
    }

    pub fn put<T: Serialize>(&self, key: &str, value: &T) {
        let fetched_at = chrono::Utc::now().timestamp();
        let envelope = CacheEnvelope { fetched_at, value };
        // Serialize value for mem cache
        let val_bytes = bincode::serialize(value).unwrap_or_default();
        let _ = self
            .mem
            .write()
            .map(|mut m| m.insert(key.to_owned(), (fetched_at, val_bytes)));

        let path = self.path_bin(key);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = bincode::serialize(&envelope) {
            let _ = fs::write(&path, bytes);
            // Remove legacy json to keep cache tidy (best effort)
            let _ = fs::remove_file(self.path_json(key));
        }
    }

    /// Put with explicit fetched_at (for testing).
    #[cfg(test)]
    pub fn put_with_time<T: Serialize>(&self, key: &str, value: &T, fetched_at: i64) {
        let envelope = CacheEnvelope { fetched_at, value };
        let val_bytes = bincode::serialize(value).unwrap_or_default();
        let _ = self
            .mem
            .write()
            .map(|mut m| m.insert(key.to_owned(), (fetched_at, val_bytes)));
        let path = self.path_bin(key);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = bincode::serialize(&envelope) {
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
        let value: Vec<u32> = cache
            .get("test/roundtrip", Duration::from_secs(60))
            .unwrap();
        assert_eq!(value, vec![1, 2, 3]);
    }
}
