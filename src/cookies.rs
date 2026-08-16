//! A JSON-persistable cookie store implementing `reqwest::cookie::CookieStore`.
//!
//! The store is shared (via `Arc`) between the redirect-following and
//! redirect-refusing HTTP clients so that both see the same session state,
//! and can be serialized to disk so sessions survive restarts.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Mutex;

use cookie::Cookie as RawCookie;
use reqwest::header::HeaderValue;
use reqwest::Url;
use serde::{Deserialize, Serialize};

/// A single stored cookie, reduced to the fields we need for matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    /// Cookie domain, stored without any leading dot.
    pub domain: String,
    /// True when the cookie was set without a Domain attribute (host-only).
    pub host_only: bool,
    pub path: String,
    /// Unix timestamp after which the cookie must be dropped, if any.
    pub expires_at: Option<i64>,
}

/// Thread-safe, serializable cookie jar.
#[derive(Debug, Default)]
pub struct PersistentCookieStore {
    /// Map keyed by `domain` (dot-free, lowercase) -> cookies by name.
    jars: Mutex<HashMap<String, HashMap<String, StoredCookie>>>,
}

impl PersistentCookieStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Restore a store previously written by [`PersistentCookieStore::snapshot`].
    /// Expired cookies are discarded on load.
    pub fn from_snapshot(cookies: Vec<StoredCookie>, now: i64) -> Self {
        let store = Self::default();
        {
            let mut jars = store.jars.lock().expect("cookie store poisoned");
            for cookie in cookies {
                if cookie.expires_at.is_some_and(|exp| exp <= now) {
                    continue;
                }
                jars.entry(cookie.domain.clone())
                    .or_default()
                    .insert(cookie.name.clone(), cookie);
            }
        }
        store
    }

    /// Snapshot all live cookies for persistence.
    pub fn snapshot(&self, now: i64) -> Vec<StoredCookie> {
        let jars = self.jars.lock().expect("cookie store poisoned");
        jars.values()
            .flat_map(|jar| {
                jar.values()
                    .filter(|cookie| cookie.expires_at.is_none_or(|exp| exp > now))
                    .cloned()
            })
            .collect()
    }

    /// Drop every cookie whose domain ends with `domain` — used on logout.
    pub fn clear_domain(&self, domain: &str) {
        let mut jars = self.jars.lock().expect("cookie store poisoned");
        jars.retain(|key, _| !key.ends_with(domain));
    }

    /// Look up a single cookie value by domain suffix and name.
    pub fn cookie_value(&self, domain_suffix: &str, name: &str) -> Option<String> {
        let jars = self.jars.lock().expect("cookie store poisoned");
        jars.iter()
            .filter(|(domain, _)| domain.ends_with(domain_suffix))
            .flat_map(|(_, jar)| jar.values())
            .find(|cookie| cookie.name == name)
            .map(|cookie| cookie.value.clone())
    }

    fn domain_matches(cookie_domain: &str, host: &str) -> bool {
        if cookie_domain == host {
            return true;
        }
        // Domain cookies (leading dot stripped) also match subdomains.
        host.ends_with(cookie_domain)
            && host.as_bytes().get(host.len() - cookie_domain.len() - 1) == Some(&b'.')
    }

    fn path_matches(cookie_path: &str, request_path: &str) -> bool {
        if cookie_path == request_path {
            return true;
        }
        if request_path.starts_with(cookie_path) {
            let tail = &request_path[cookie_path.len()..];
            return cookie_path.ends_with('/') || tail.starts_with('/');
        }
        false
    }
}

impl reqwest::cookie::CookieStore for PersistentCookieStore {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let mut jars = self.jars.lock().expect("cookie store poisoned");
        for header in cookie_headers {
            let Ok(text) = header.to_str() else {
                continue;
            };
            let Ok(raw) = RawCookie::parse(Cow::Borrowed(text)) else {
                continue;
            };
            let name = raw.name().to_owned();

            // Derive the storage domain: explicit Domain attribute, else the host.
            let host_only = raw.domain().is_none();
            let storage_domain = match raw.domain() {
                Some(domain) => domain.trim_start_matches('.').to_lowercase(),
                None => url.host_str().unwrap_or_default().to_lowercase(),
            };
            if storage_domain.is_empty() {
                continue;
            }

            let entry = jars.entry(storage_domain.clone()).or_default();
            if raw.value().is_empty() {
                // Empty value means the server asked us to delete the cookie.
                entry.remove(&name);
                continue;
            }

            let stored = StoredCookie {
                name,
                value: raw.value().to_owned(),
                domain: storage_domain,
                host_only,
                path: raw.path().unwrap_or("/").to_owned(),
                expires_at: raw.expires_datetime().map(|dt| dt.unix_timestamp()),
            };
            entry.insert(stored.name.clone(), stored);
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let now = chrono::Utc::now().timestamp();
        let host = url.host_str()?.to_lowercase();
        let request_path = if url.path().is_empty() { "/" } else { url.path() };

        let mut jars = self.jars.lock().expect("cookie store poisoned");
        let mut pairs: Vec<String> = Vec::new();
        for (domain, jar) in jars.iter_mut() {
            if !Self::domain_matches(domain, &host) {
                continue;
            }
            jar.retain(|_, cookie| cookie.expires_at.is_none_or(|exp| exp > now));
            for cookie in jar.values() {
                if cookie.host_only && domain.as_str() != host {
                    continue;
                }
                if !Self::path_matches(&cookie.path, request_path) {
                    continue;
                }
                pairs.push(format!("{}={}", cookie.name, cookie.value));
            }
        }
        if pairs.is_empty() {
            return None;
        }
        HeaderValue::from_str(&pairs.join("; ")).ok()
    }
}
