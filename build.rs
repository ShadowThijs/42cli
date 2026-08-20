fn main() {
    println!("cargo:rerun-if-env-changed=CLI42_BUILD_TAG");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    let version = detect_version();
    println!("cargo:rustc-env=CLI42_VERSION={version}");
    // Rerun if git state changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags/");
}

fn detect_version() -> String {
    // 1. If built from a tag (CI), GITHUB_REF_NAME style env may contain it.
    // 2. Try git describe --tags --always
    if let Ok(tag) = std::env::var("CLI42_BUILD_TAG") {
        let t = tag.trim().to_owned();
        if !t.is_empty() {
            return t;
        }
    }
    // Try git describe
    if let Ok(output) = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        && output.status.success()
    {
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !raw.is_empty() {
            // If it's a tag like v1.2.3 or 1.2.3, return as-is.
            // If it's a commit hash, prefix with dev-.
            if raw.starts_with('v') || raw.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                // Check if it looks like a version (contains dot)
                if raw.contains('.') {
                    return raw;
                }
            }
            // commit hash fallback
            // Try to get nearest tag + commit: git describe may already give tag-commit
            // For plain hash, mark as dev-
            if raw.len() >= 7 && raw.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                return format!("dev-{raw}");
            }
            return raw;
        }
    }
    // Try plain git rev-parse --short HEAD
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        && output.status.success()
    {
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !hash.is_empty() {
            return format!("dev-{hash}");
        }
    }
    // Fallback to Cargo.toml version or unknown
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".into())
}
