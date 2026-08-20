//! Encrypted vault for tokens + credentials.
//!
//! The vault lives at `$XDG_CACHE_HOME/42cli/vault.enc` and is encrypted
//! with a 32-byte master key at `$XDG_CONFIG_HOME/42cli/.master.key` (0600).
//! Encryption is ChaCha20Poly1305 with a random 12-byte nonce per write.
//! The file is `nonce || ciphertext` where ciphertext is the bincode of
//! [`Vault`] encrypted with AEAD.
//!
//! Legacy `~/.config/42cli/session.json` (plain JSON) is migrated on first
//! load and then securely erased.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use serde::{Deserialize, Serialize};

use crate::cookies::StoredCookie;

// ---------------------------------------------------------------- vault --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    pub username: String,
    /// Plain password stored encrypted at rest; empty if not yet saved.
    pub password: String,
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: i64,
    pub login: String,
    pub user_id: u32,
    pub cookies: Vec<StoredCookie>,
}

// -------------------------------------------------------------- paths ----

fn master_key_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42cli")
        .join(".master.key")
}

fn vault_path() -> PathBuf {
    crate::config::cache_dir().join("vault.enc")
}

fn legacy_session_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("42cli")
        .join("session.json")
}

// --------------------------------------------------------- master key ----

fn load_or_create_master_key() -> Result<[u8; 32]> {
    let path = master_key_path();
    if let Ok(bytes) = fs::read(&path)
        && bytes.len() == 32
    {
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }
    // Corrupt key: regenerate
    let mut key = [0u8; 32];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create config dir for master key")?;
    }
    fs::write(&path, key).context("write master key")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

fn cipher(key_bytes: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(Key::from_slice(key_bytes))
}

// ----------------------------------------------------------- encrypt ----

fn encrypt_vault(vault: &Vault, key: &[u8; 32]) -> Result<Vec<u8>> {
    let plain = bincode::serialize(vault).context("bincode vault")?;
    let mut nonce_bytes = [0u8; 12];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher(key)
        .encrypt(nonce, plain.as_ref())
        .map_err(|_| anyhow::anyhow!("encrypt failed"))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_vault(data: &[u8], key: &[u8; 32]) -> Result<Vault> {
    if data.len() < 12 {
        anyhow::bail!("vault too short");
    }
    let (nonce_bytes, ct) = data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher(key)
        .decrypt(nonce, ct)
        .map_err(|_| anyhow::anyhow!("decrypt failed"))?;
    bincode::deserialize(&plain).context("bincode decode vault")
}

// ----------------------------------------------------------- helpers ----

fn secure_erase(path: &PathBuf) {
    if !path.exists() {
        return;
    }
    // Overwrite with zeros before unlink to avoid plain JSON lingering on disk.
    if let Ok(meta) = fs::metadata(path) {
        let len = meta.len() as usize;
        if len > 0 && len < 10_000_000 {
            let zeros = vec![0u8; len];
            let _ = fs::write(path, &zeros);
        }
    }
    let _ = fs::remove_file(path);
}

// -------------------------------------------------------------- public ----

/// Load the vault, migrating legacy `session.json` if needed.
pub fn load_vault() -> Option<Vault> {
    // Try encrypted vault first.
    if let Ok(key) = load_or_create_master_key()
        && let Ok(data) = fs::read(vault_path())
        && let Ok(vault) = decrypt_vault(&data, &key)
    {
        // If legacy still exists, erase it now that vault is valid.
        let legacy = legacy_session_path();
        if legacy.exists() {
            secure_erase(&legacy);
        }
        return Some(vault);
    }
    // No vault -> try legacy migration.
    if let Some(migrated) = try_migrate_legacy() {
        // Persist migrated vault (encrypted) and erase legacy.
        let _ = save_vault(&migrated);
        secure_erase(&legacy_session_path());
        return Some(migrated);
    }
    None
}

fn try_migrate_legacy() -> Option<Vault> {
    let bytes = fs::read(legacy_session_path()).ok()?;
    // Legacy shape is StoredSession
    let legacy: crate::config::StoredSession = serde_json::from_slice(&bytes).ok()?;
    Some(Vault {
        username: legacy.login.clone(),
        password: String::new(),
        access_token: legacy.access_token,
        refresh_token: legacy.refresh_token,
        access_expires_at: legacy.access_expires_at,
        login: legacy.login,
        user_id: legacy.user_id,
        cookies: legacy.cookies,
    })
}

/// Persist vault encrypted. Creates cache dir and sets 0600 on file.
pub fn save_vault(vault: &Vault) -> Result<()> {
    let key = load_or_create_master_key()?;
    let data = encrypt_vault(vault, &key)?;
    let path = vault_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create cache dir")?;
    }
    fs::write(&path, &data).context("write vault")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    // Ensure legacy is gone.
    let legacy = legacy_session_path();
    if legacy.exists() {
        secure_erase(&legacy);
    }
    Ok(())
}

pub fn clear_vault() {
    secure_erase(&vault_path());
    // Also clear legacy if somehow still there.
    secure_erase(&legacy_session_path());
    // Keep master key (reused for next vault) — optional to nuke it too.
}

/// Update only credentials inside existing vault (or create new if none).
pub fn save_credentials(username: &str, password: &str) -> Result<()> {
    let mut vault = load_vault().unwrap_or_else(|| Vault {
        username: username.to_owned(),
        password: password.to_owned(),
        access_token: String::new(),
        refresh_token: String::new(),
        access_expires_at: 0,
        login: String::new(),
        user_id: 0,
        cookies: Vec::new(),
    });
    vault.username = username.to_owned();
    vault.password = password.to_owned();
    save_vault(&vault)
}

/// Convert vault to StoredSession for existing codepaths that expect it.
pub fn vault_to_stored(vault: &Vault) -> crate::config::StoredSession {
    crate::config::StoredSession {
        access_token: vault.access_token.clone(),
        refresh_token: vault.refresh_token.clone(),
        access_expires_at: vault.access_expires_at,
        login: vault.login.clone(),
        user_id: vault.user_id,
        cookies: vault.cookies.clone(),
    }
}

pub fn stored_to_vault(
    stored: &crate::config::StoredSession,
    username: &str,
    password: &str,
) -> Vault {
    Vault {
        username: username.to_owned(),
        password: password.to_owned(),
        access_token: stored.access_token.clone(),
        refresh_token: stored.refresh_token.clone(),
        access_expires_at: stored.access_expires_at,
        login: stored.login.clone(),
        user_id: stored.user_id,
        cookies: stored.cookies.clone(),
    }
}
