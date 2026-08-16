//! HTTP client plumbing shared by every 42 endpoint family.

pub mod auth;
pub mod aux;
pub mod error;
pub mod intrapy;
pub mod models;
pub mod slots;
pub mod web;

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, ClientBuilder, Url};
use tokio::sync::RwLock;

use crate::cache::DiskCache;
use crate::cookies::PersistentCookieStore;

pub use error::{ApiError, ApiResult};
pub use models::*;

pub const KEYCLOAK_BASE: &str = "https://auth.42.fr/auth/realms/students-42";
pub const FRONTEND_CLIENT: &str = "frontend-react";
pub const PROFILE_V3_REDIRECT: &str = "https://profile-v3.intra.42.fr";

pub const INTRAPY_BASE: &str = "https://intrapy.intra.42.fr/api/v1";
pub const TRANSLATE_BASE: &str = "https://translate.intra.42.fr";
pub const EDTRAX_BASE: &str = "https://edtrax.42.fr/api/v1";
pub const PACE_BASE: &str = "https://pace-system.42.fr/api/v1";
pub const PROJECTS_BASE: &str = "https://projects.intra.42.fr";
pub const PROFILE_BASE: &str = "https://profile.intra.42.fr";
pub const META_BASE: &str = "https://meta.intra.42.fr";
pub const CDN_BASE: &str = "https://cdn.intra.42.fr";

pub const SLOTS_BASE: &str = "https://slots.42belgium.be";
pub const ACCOUNTS_BASE: &str = "https://accounts.42belgium.be";

const USER_AGENT: &str = concat!("42cli/", env!("CARGO_PKG_VERSION"));

/// Shared API state: two clients (redirect / no-redirect) over one cookie
/// store, plus the OAuth token set and the disk cache.
pub struct Api {
    /// Follows redirects — used for session bootstrap chains and plain GETs.
    pub http: Client,
    /// Refuses redirects — used where we must inspect intermediate hops.
    pub noredirect: Client,
    pub cookies: Arc<PersistentCookieStore>,
    pub cache: DiskCache,
    tokens: RwLock<Option<TokenSet>>,
}

impl Api {
    pub fn new(cookies: Arc<PersistentCookieStore>, tokens: Option<TokenSet>) -> ApiResult<Self> {
        let http = client_builder(&cookies)?.build()?;
        let noredirect = client_builder(&cookies)?
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            noredirect,
            cookies,
            cache: DiskCache::new(),
            tokens: RwLock::new(tokens),
        })
    }

    pub async fn tokens(&self) -> Option<TokenSet> {
        self.tokens.read().await.clone()
    }

    pub async fn set_tokens(&self, set: TokenSet) {
        *self.tokens.write().await = Some(set);
    }

    pub async fn clear_tokens(&self) {
        *self.tokens.write().await = None;
    }

    /// CSRF token for the slots site: Django accepts the raw `csrftoken`
    /// cookie value in the `X-CSRFToken` header.
    pub fn slots_csrf(&self) -> Option<String> {
        self.cookies.cookie_value("slots.42belgium.be", "csrftoken")
    }

    /// GET a bearer-authenticated JSON endpoint, transparently refreshing
    /// the access token once when it has expired.
    pub async fn authed_get<T: serde::de::DeserializeOwned>(&self, url: &str) -> ApiResult<T> {
        match self.try_authed_get::<T>(url, false).await {
            Ok(value) => Ok(value),
            Err(ApiError::SessionExpired) => {
                self.refresh().await?;
                self.try_authed_get::<T>(url, true).await
            }
            Err(error) => Err(error),
        }
    }

    async fn try_authed_get<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        _retry: bool,
    ) -> ApiResult<T> {
        let tokens = self
            .tokens()
            .await
            .ok_or(ApiError::SessionExpired)?;
        let resp = self
            .http
            .get(url)
            .bearer_auth(tokens.access_token)
            .header("content-type", "application/json")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ApiError::from_response("api", resp).await);
        }
        resp.json::<T>()
            .await
            .map_err(|error| ApiError::Parse {
                endpoint: "api",
                detail: error.to_string(),
            })
    }

    /// Refresh the access token via the stored refresh token.
    pub async fn refresh(&self) -> ApiResult<TokenSet> {
        let refresh_token = self
            .tokens()
            .await
            .map(|set| set.refresh_token)
            .ok_or(ApiError::SessionExpired)?;
        let set = auth::refresh_tokens(&self.http, &refresh_token).await?;
        self.set_tokens(set.clone()).await;
        auth::persist_session(self).await;
        Ok(set)
    }

    /// Session cookie for intra.42.fr, used to detect web-session validity.
    pub fn has_intra_session(&self) -> bool {
        self.cookies
            .cookie_value("intra.42.fr", "_intra_42_session_production")
            .is_some()
    }

    pub fn has_slots_session(&self) -> bool {
        self.cookies
            .cookie_value("42belgium.be", "sessionid")
            .is_some()
    }
}

fn client_builder(cookies: &Arc<PersistentCookieStore>) -> ApiResult<ClientBuilder> {
    Ok(Client::builder()
        .cookie_provider(Arc::clone(cookies))
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .gzip(true))
}

/// Absolute URL join helper (avoids pulling in `Url` everywhere).
pub fn url_join(base: &str, path: &str) -> Url {
    Url::parse(base)
        .and_then(|base| base.join(path))
        .unwrap_or_else(|_| Url::parse(path).expect("static url"))
}
