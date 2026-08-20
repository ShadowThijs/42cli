//! 42cli — a Ratatui client for the 42 intranet and 42 Belgium slots.

mod api;
mod app;
mod bus;
mod cache;
mod config;
mod cookies;
mod event;
mod input;
mod msg;
mod pdfmd;
mod secure;
mod state;
mod ui;
mod update;
mod util;
mod worker;

#[cfg(test)]
mod tests_live;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    // Subcommands that run without the TUI.
    match std::env::args().nth(1).as_deref() {
        Some("install") => return install_userwide(),
        Some("--version") | Some("-v") | Some("version") => {
            println!("42cli {}", update::VERSION);
            return Ok(());
        }
        Some("update") => return run_update(),
        Some("help") | Some("--help") | Some("-h") => {
            println!("42cli — Ratatui client for the 42 intranet and 42 Belgium slots");
            println!();
            println!("Usage: cli42 [command]");
            println!();
            println!("  (no args)   start the TUI");
            println!("  install     copy this binary to ~/.local/bin and check PATH");
            println!("  update      fetch latest release from GitHub and self-replace");
            println!("  --version   print baked version");
            return Ok(());
        }
        Some(other) => {
            anyhow::bail!("unknown subcommand `{other}` — try `cli42 help`");
        }
        None => {}
    }

    // Cheap auto-update check: if a cached check says newer exists, remember it for banner.
    // Full network check runs in background thread without blocking startup.
    let cached_update = update::cached_update_available();

    let cookies = restore_cookies();
    let api = Arc::new(api::Api::new(cookies.clone(), None)?);
    let (cmd_tx, msg_rx, worker) = bus::spawn_worker(api);
    let mut app = app::App::new(cmd_tx);
    if let Some(tag) = cached_update {
        app.update_available = Some(tag);
    }
    // Background update check (non-blocking) — writes sentinel file polled by event loop.
    std::thread::spawn(|| {
        if let Some(tag) = update::check_for_update_blocking() {
            let path = config::cache_dir().join("update_available");
            let _ = std::fs::write(path, tag);
        }
    });

    // Try a stored session before showing the login form.
    // Optimistic: if a vault exists, presume token valid and drop straight to Main
    // with cached profile data, correcting in background after verification.
    if let Some(stored) = config::load_session() {
        // Hydrate from cache before any network
        app.hydrate_from_cache();
        // Decide screen: if we have at least a cached summary, go Main optimistically
        if app.dash.summary.data().is_some() {
            app.screen = app::Screen::Main;
            app.tab = app::Tab::Dashboard;
            app.set_status(format!("welcome back, {}", stored.login));
        } else {
            // No cached profile — show loading but still go Main to avoid login flash
            app.screen = app::Screen::Main;
            app.tab = app::Tab::Dashboard;
            app.dash.summary = state::Loadable::Loading;
            // Keep login state loading for fallback
            app.login.state = state::Loadable::Loading;
        }
        app.send(bus::Command::Restore(stored));
    }

    let mut terminal = event::setup_terminal()?;
    let run_result = event::run(&mut terminal, &mut app, msg_rx);
    event::restore_terminal(&mut terminal)?;
    // An editor picked in the post-clone popup: exec() replaces this very
    // process with `editor .` inside the cloned folder, so quitting the
    // editor drops you into the shell exactly like running it by hand.
    let editor = app.pending_editor.take();
    // Drop the app first: it owns the command sender, and the worker loop
    // only ends once that channel closes — otherwise `join` blocks forever.
    drop(app);
    let _ = worker.join();
    run_result?;
    if let Some((editor, path)) = editor {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new(&editor)
            .arg(".")
            .current_dir(&path)
            .exec();
        // exec() only ever returns on failure.
        anyhow::bail!("cannot launch {editor} in {path}: {error}");
    }
    Ok(())
}

/// Copy the running binary into `~/.local/bin` and tell the user whether
/// that directory is on their `$PATH` (with the exact line to add if not).
fn install_userwide() -> Result<()> {
    let home = dirs::home_dir().context("cannot find your home directory")?;
    let bin_dir = home.join(".local").join("bin");
    std::fs::create_dir_all(&bin_dir).context("create ~/.local/bin")?;

    let exe = std::env::current_exe().context("locate the running binary")?;
    let dest = bin_dir.join("cli42");
    std::fs::copy(&exe, &dest).with_context(|| format!("copy to {}", dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    println!("installed: {}", dest.display());

    let path = std::env::var_os("PATH").unwrap_or_default();
    let listed = path
        .to_string_lossy()
        .split(':')
        .any(|entry| Path::new(entry) == bin_dir);
    if listed {
        println!("~/.local/bin is already on your PATH — run `cli42` from anywhere.");
    } else {
        println!("~/.local/bin is NOT on your PATH for this shell.");
        println!("Add this line to ~/.bashrc (or ~/.zshrc) and reopen the terminal:");
        println!("  export PATH=\"$HOME/.local/bin:$PATH\"");
    }
    Ok(())
}

fn restore_cookies() -> Arc<cookies::PersistentCookieStore> {
    match config::load_session() {
        Some(stored) => Arc::new(cookies::PersistentCookieStore::from_snapshot(
            stored.cookies,
            chrono::Utc::now().timestamp(),
        )),
        None => Arc::new(cookies::PersistentCookieStore::new()),
    }
}

fn run_update() -> Result<()> {
    println!("current version: {}", update::VERSION);
    println!("checking for updates...");
    match update::perform_update() {
        Ok(msg) => {
            println!("{msg}");
            Ok(())
        }
        Err(e) => anyhow::bail!("update failed: {e}"),
    }
}
