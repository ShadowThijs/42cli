//! Version baking and auto-update.
//!
//! Version is baked at compile time via `build.rs` into env `CLI42_VERSION`.
//! Update checks GitHub releases API for `ShadowThijs/42cli`.

use std::path::Path;

pub const VERSION: &str = env!("CLI42_VERSION");
const REPO: &str = "ShadowThijs/42cli";
const CHECK_TTL_SECS: i64 = 6 * 3600; // 6 hours

#[derive(Debug, serde::Deserialize)]
struct Release {
    tag_name: String,
}

fn cache_path() -> std::path::PathBuf {
    crate::config::cache_dir().join("update_check.bin")
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CheckCache {
    checked_at: i64,
    latest_tag: String,
}

fn normalize_tag(tag: &str) -> &str {
    tag.trim_start_matches('v')
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    // Simple semver compare: split by '.' and compare numerically.
    if latest == current {
        return false;
    }
    // If current is dev-*, always consider latest newer
    if current.starts_with("dev-") || current == "unknown" {
        return true;
    }
    let parse = |v: &str| {
        normalize_tag(v)
            .split(['.', '-'])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect::<Vec<_>>()
    };
    let l = parse(latest);
    let c = parse(current);
    // If we can't parse, do string compare
    if l.is_empty() || c.is_empty() {
        return normalize_tag(latest) != normalize_tag(current);
    }
    l > c
}

pub fn cached_update_available() -> Option<String> {
    let bytes = std::fs::read(cache_path()).ok()?;
    let cached: CheckCache = bincode::deserialize(&bytes).ok()?;
    let age = chrono::Utc::now().timestamp() - cached.checked_at;
    if age > CHECK_TTL_SECS {
        return None;
    }
    if is_newer(&cached.latest_tag, VERSION) {
        Some(cached.latest_tag)
    } else {
        None
    }
}

/// Blocking check (called from background thread).
pub fn check_for_update_blocking() -> Option<String> {
    // Respect TTL to avoid hammering GitHub API.
    if let Ok(bytes) = std::fs::read(cache_path())
        && let Ok(cached) = bincode::deserialize::<CheckCache>(&bytes)
    {
        let age = chrono::Utc::now().timestamp() - cached.checked_at;
        if age < CHECK_TTL_SECS {
            if is_newer(&cached.latest_tag, VERSION) {
                return Some(cached.latest_tag);
            } else {
                return None;
            }
        }
    }
    let latest = fetch_latest_tag_blocking()?;
    let cache = CheckCache {
        checked_at: chrono::Utc::now().timestamp(),
        latest_tag: latest.clone(),
    };
    if let Ok(bytes) = bincode::serialize(&cache) {
        let _ = std::fs::write(cache_path(), bytes);
    }
    if is_newer(&latest, VERSION) {
        Some(latest)
    } else {
        None
    }
}

fn fetch_latest_tag_blocking() -> Option<String> {
    // Use reqwest blocking client via std process curl? Use reqwest blocking feature not enabled.
    // Use simple ureq-like via `reqwest` async? We'll spawn a tiny tokio runtime.
    // Simpler: use `curl` via command if available, else try reqwest via blocking via `std::process::Command` with `wget`?
    // We'll try to use `reqwest` blocking via creating a runtime.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    rt.block_on(fetch_latest_tag_async())
}

async fn fetch_latest_tag_async() -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("42cli/{}", VERSION))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let rel: Release = resp.json().await.ok()?;
    Some(rel.tag_name)
}

/// Perform update: download latest binary and replace current exe.
pub fn perform_update() -> anyhow::Result<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(perform_update_async())
}

async fn perform_update_async() -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("42cli/{}", VERSION))
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("GitHub API: {}", resp.status());
    }
    let rel: Release = resp.json().await?;
    let tag = rel.tag_name.clone();
    if !is_newer(&tag, VERSION) {
        return Ok(format!("already on latest ({VERSION})"));
    }
    // Download binary: `https://github.com/{REPO}/releases/download/{tag}/cli42`
    let download_url = format!("https://github.com/{REPO}/releases/download/{tag}/cli42");
    let resp = client.get(&download_url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await?;
    let exe = std::env::current_exe()?;
    let dest_tmp = exe.with_extension("tmp");
    std::fs::write(&dest_tmp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest_tmp, std::fs::Permissions::from_mode(0o755));
    }
    // Atomic replace
    self_replace(&dest_tmp, &exe)?;
    // Update cache
    let cache = CheckCache {
        checked_at: chrono::Utc::now().timestamp(),
        latest_tag: tag.clone(),
    };
    if let Ok(b) = bincode::serialize(&cache) {
        let _ = std::fs::write(cache_path(), b);
    }
    Ok(format!("updated to {tag}"))
}

#[cfg(unix)]
fn self_replace(src: &Path, dst: &Path) -> anyhow::Result<()> {
    // Try rename, fallback to copy.
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst)?;
    let _ = std::fs::remove_file(src);
    Ok(())
}
#[cfg(not(unix))]
fn self_replace(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::copy(src, dst)?;
    let _ = std::fs::remove_file(src);
    Ok(())
}
