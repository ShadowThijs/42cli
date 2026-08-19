//! Application paths and session persistence.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::cookies::StoredCookie;

/// Everything that must survive a restart, stored as JSON with `0o600` perms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds since epoch when the access token expires.
    pub access_expires_at: i64,
    pub login: String,
    pub user_id: u32,
    pub cookies: Vec<StoredCookie>,
}

/// Resolve the config file path (`$XDG_CONFIG_HOME/42cli/session.json`).
pub fn session_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42cli")
        .join("session.json")
}

/// Resolve the cache directory (`$XDG_CACHE_HOME/42cli`), creating it if needed.
pub fn cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42cli");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Directory where downloaded project documents are written.
pub fn downloads_dir() -> PathBuf {
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42cli-documents");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn load_session() -> Option<StoredSession> {
    let bytes = fs::read(session_path()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_session(session: &StoredSession) -> Result<()> {
    let path = session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config dir")?;
    }
    fs::write(&path, serde_json::to_vec_pretty(session)?).context("write session file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn clear_session() {
    let _ = fs::remove_file(session_path());
}

/// User preferences that survive restarts (unlike the session, these are
/// not secret): `~/.config/42cli/settings.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Last used `g` clone destination directory.
    pub clone_dest: Option<String>,
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42cli")
        .join("settings.json")
}

pub fn load_settings() -> Settings {
    fs::read(settings_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config dir")?;
    }
    fs::write(&path, serde_json::to_vec_pretty(settings)?).context("write settings file")?;
    Ok(())
}
