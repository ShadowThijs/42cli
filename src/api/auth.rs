//! Authentication: Keycloak OIDC (PKCE) for the intrapy bearer API,
//! intra.42.fr Rails session bootstrap, and the 42 Belgium slots session.

use base64::Engine as _;
use rand::Rng;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    Api, ApiError, ApiResult, FRONTEND_CLIENT, KEYCLOAK_BASE, PROFILE_V3_REDIRECT, TokenSet,
};
use crate::config::{self, StoredSession};

const INTRA_OAUTH_CALLBACK: &str =
    "https://profile.intra.42.fr/users/auth/keycloak_student/callback";
const SLOTS_NEXT: &str = "https://slots.42belgium.be/slots";

/// Result of a successful interactive login.
#[derive(Debug, Clone)]
pub struct LoginOutcome {
    pub login: String,
    pub user_id: u32,
}

// ------------------------------------------------------------- PKCE ----

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn pkce_pair() -> (String, String) {
    let verifier = random_hex(48);
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

// ------------------------------------------------------------ login ----

/// Run the full headless login: Keycloak credentials -> tokens -> intra
/// session -> 42 Belgium slots session.
pub async fn login(api: &Api, username: &str, password: &str) -> ApiResult<LoginOutcome> {
    // 1. Ask Keycloak for the login form (fresh PKCE pair per attempt).
    let (verifier, challenge) = pkce_pair();
    let state = random_hex(16);
    let authorize_url = format!(
        "{KEYCLOAK_BASE}/protocol/openid-connect/auth\
         ?client_id={FRONTEND_CLIENT}\
         &redirect_uri={PROFILE_V3_REDIRECT}\
         &response_type=code&scope=openid%20profile%20email\
         &state={state}&code_challenge={challenge}&code_challenge_method=S256\
         &response_mode=query"
    );
    let resp = api.noredirect.get(&authorize_url).send().await?;
    let code = if resp.status().is_redirection() {
        // Existing SSO session: the code comes straight back.
        let location = redirect_location(&resp)?;
        extract_query_param(&location, "code")?
    } else {
        let html = resp.text().await.unwrap_or_default();
        let action = parse_login_form_action(&html).ok_or_else(|| ApiError::Parse {
            endpoint: "keycloak",
            detail: "no login form".into(),
        })?;
        // 2. Submit credentials; Keycloak answers 302 with `code` on success.
        let resp = api
            .noredirect
            .post(action)
            .form(&[
                ("username", username),
                ("password", password),
                ("credentialId", ""),
                ("login", "Sign In"),
                ("rememberMe", "on"),
            ])
            .send()
            .await?;
        if !resp.status().is_redirection() {
            return Err(ApiError::BadCredentials);
        }
        let location = redirect_location(&resp)?;
        extract_query_param(&location, "code")?
    };

    // 3. Exchange the authorization code for tokens.
    let tokens = exchange_code(api, &verifier, &code).await?;
    api.set_tokens(tokens).await;

    // 4. Ride the Keycloak SSO cookie into an intra.42.fr Rails session.
    bootstrap_intra_session(api).await;

    // 5. And through intra OAuth into the 42 Belgium slots session.
    bootstrap_slots_session(api).await;

    let tokens = api.tokens().await.expect("tokens were just set");
    let (login, user_id) = decode_jwt_identity(&tokens.access_token)?;
    // Save credentials securely for auto-relogin.
    persist_credentials(username, password);
    persist_session(api).await;

    Ok(LoginOutcome { login, user_id })
}

fn redirect_location(resp: &reqwest::Response) -> ApiResult<String> {
    resp.headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| ApiError::Parse {
            endpoint: "keycloak",
            detail: "redirect without Location".into(),
        })
}

fn extract_query_param(url: &str, key: &str) -> ApiResult<String> {
    url.split_once('?')
        .map(|(_, query)| query)
        .ok_or_else(|| ApiError::Parse {
            endpoint: "keycloak",
            detail: format!("redirect without query: {url}"),
        })?
        .split('&')
        .find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == key))
        .map(|(_, v)| v.to_owned())
        .ok_or_else(|| ApiError::Parse {
            endpoint: "keycloak",
            detail: format!("`{key}` missing in redirect"),
        })
}

/// The Keycloak login page posts to a one-shot URL embedded in the form.
fn parse_login_form_action(html: &str) -> Option<String> {
    use scraper::{Html, Selector};
    let document = Html::parse_document(html);
    let selector = Selector::parse("form#kc-form-login").ok()?;
    document
        .select(&selector)
        .next()?
        .attr("action")
        .map(str::to_owned)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

async fn exchange_code(api: &Api, verifier: &str, code: &str) -> ApiResult<TokenSet> {
    let resp = api
        .http
        .post(format!("{KEYCLOAK_BASE}/protocol/openid-connect/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("redirect_uri", PROFILE_V3_REDIRECT),
            ("code", code),
            ("code_verifier", verifier),
            ("client_id", FRONTEND_CLIENT),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(ApiError::from_response("token", resp).await);
    }
    let body: TokenResponse = resp.json().await.map_err(|error| ApiError::Parse {
        endpoint: "token",
        detail: error.to_string(),
    })?;
    Ok(TokenSet {
        access_expires_at: chrono::Utc::now().timestamp() + body.expires_in - 10,
        access_token: body.access_token,
        refresh_token: body.refresh_token,
    })
}

pub(super) async fn refresh_tokens(
    http: &reqwest::Client,
    refresh_token: &str,
) -> ApiResult<TokenSet> {
    let resp = http
        .post(format!("{KEYCLOAK_BASE}/protocol/openid-connect/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", FRONTEND_CLIENT),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(ApiError::SessionExpired);
    }
    let body: TokenResponse = resp.json().await.map_err(|error| ApiError::Parse {
        endpoint: "refresh",
        detail: error.to_string(),
    })?;
    Ok(TokenSet {
        access_expires_at: chrono::Utc::now().timestamp() + body.expires_in - 10,
        access_token: body.access_token,
        refresh_token: body.refresh_token,
    })
}

/// Pull `preferred_username` / `user_id` out of the access token payload.
fn decode_jwt_identity(token: &str) -> ApiResult<(String, u32)> {
    let payload_b64 = token.split('.').nth(1).ok_or_else(|| ApiError::Parse {
        endpoint: "jwt",
        detail: "no payload".into(),
    })?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|error| ApiError::Parse {
            endpoint: "jwt",
            detail: error.to_string(),
        })?;
    let payload: Value = serde_json::from_slice(&payload).map_err(|error| ApiError::Parse {
        endpoint: "jwt",
        detail: error.to_string(),
    })?;
    let login = payload["preferred_username"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let user_id = payload["user_id"].as_u64().unwrap_or_default() as u32;
    if login.is_empty() {
        return Err(ApiError::Parse {
            endpoint: "jwt",
            detail: "no username".into(),
        });
    }
    Ok((login, user_id))
}

// --------------------------------------------------------- bootstrap ----

/// Convert the Keycloak SSO cookie into an intra.42.fr session cookie by
/// walking the `client_id=intra` OAuth redirect chain.
pub async fn bootstrap_intra_session(api: &Api) -> bool {
    let state = random_hex(16);
    let url = format!(
        "{KEYCLOAK_BASE}/protocol/openid-connect/auth\
         ?client_id=intra&redirect_uri={INTRA_OAUTH_CALLBACK}\
         &response_type=code&state={state}"
    );
    matches!(api.http.get(&url).send().await, Ok(resp) if resp.status().is_success())
        && api.has_intra_session()
}

/// Walk the accounts.42belgium.be OAuth chain to obtain the shared
/// `.42belgium.be` session cookie used by the slots site.
pub async fn bootstrap_slots_session(api: &Api) -> bool {
    let _ = api
        .http
        .get(format!("{}/login/?next={SLOTS_NEXT}", super::ACCOUNTS_BASE))
        .send()
        .await;
    let Ok(resp) = api
        .noredirect
        .get(format!("{}/authenticate/", super::ACCOUNTS_BASE))
        .send()
        .await
    else {
        return false;
    };
    let Some(location) = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
    else {
        return false;
    };
    // Follow through api.intra.42.fr/oauth/authorize back to accounts.42belgium.be.
    let _ = api.http.get(&location).send().await;
    let _ = api.http.get(super::SLOTS_BASE).send().await;
    api.has_slots_session()
}

// ----------------------------------------------------------- persist ----

/// Write tokens + cookies to the session file (called after every change).
pub async fn persist_session(api: &Api) {
    let Some(tokens) = api.tokens().await else {
        return;
    };
    let identity = decode_jwt_identity(&tokens.access_token).ok();
    let session = StoredSession {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        access_expires_at: tokens.access_expires_at,
        login: identity
            .as_ref()
            .map_or_else(String::new, |(l, _)| l.clone()),
        user_id: identity.as_ref().map_or(0, |(_, id)| *id),
        cookies: api.cookies.snapshot(chrono::Utc::now().timestamp()),
    };
    // Merge existing credentials from vault if present.
    let vault_opt = crate::secure::load_vault();
    let vault = if let Some(mut v) = vault_opt {
        v.access_token = session.access_token.clone();
        v.refresh_token = session.refresh_token.clone();
        v.access_expires_at = session.access_expires_at;
        v.login = session.login.clone();
        v.user_id = session.user_id;
        v.cookies = session.cookies.clone();
        v
    } else {
        crate::secure::stored_to_vault(&session, &session.login, "")
    };
    if let Err(error) = crate::secure::save_vault(&vault) {
        tracing_note(&error.to_string());
    }
}

/// Persist credentials alongside the session (called after successful login).
pub fn persist_credentials(username: &str, password: &str) {
    let _ = crate::secure::save_credentials(username, password);
    // Also update vault's username field if vault exists.
    if let Some(mut v) = crate::secure::load_vault() {
        v.username = username.to_owned();
        v.password = password.to_owned();
        let _ = crate::secure::save_vault(&v);
    }
}

fn tracing_note(message: &str) {
    // Kept minimal on purpose: the TUI surfaces errors via the status line.
    let _ = std::fs::write(std::env::temp_dir().join("42cli-last-error.log"), message);
}

/// Forget everything: cookies, tokens, session file.
pub async fn logout(api: &Api) {
    api.clear_tokens().await;
    api.cookies.clear_domain("42.fr");
    api.cookies.clear_domain("42belgium.be");
    config::clear_session();
}
