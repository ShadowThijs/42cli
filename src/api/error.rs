//! Unified API error type.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),

    #[error("bad credentials")]
    BadCredentials,

    #[error("session expired, please log in again")]
    SessionExpired,

    #[error("{0}")]
    Status(String),

    #[error("unexpected response from {endpoint}: {detail}")]
    Parse {
        endpoint: &'static str,
        detail: String,
    },

    #[error("missing CSRF token for slots.42belgium.be — log in again")]
    MissingCsrf,

    #[error("{0}")]
    Other(String),
}

impl ApiError {
    /// Turn an HTTP response into a readable error (consuming the body).
    pub async fn from_response(endpoint: &'static str, resp: reqwest::Response) -> Self {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let body = body.trim().chars().take(240).collect::<String>();
        match status.as_u16() {
            401 => Self::SessionExpired,
            403 if body.contains("credentials") => Self::BadCredentials,
            _ => Self::Status(format!("{endpoint}: HTTP {status} {body}")),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
