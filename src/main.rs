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
mod state;
mod ui;
mod util;
mod worker;

use std::sync::Arc;

use anyhow::Result;

fn main() -> Result<()> {
    let cookies = restore_cookies();
    let api = Arc::new(api::Api::new(cookies.clone(), None)?);
    let (cmd_tx, msg_rx, worker) = bus::spawn_worker(api);
    let mut app = app::App::new(cmd_tx);

    // Try a stored session before showing the login form.
    if let Some(stored) = config::load_session() {
        app.login.state = state::Loadable::Loading;
        app.send(bus::Command::Restore(stored));
    }

    let mut terminal = event::setup_terminal()?;
    let run_result = event::run(&mut terminal, &mut app, msg_rx);
    event::restore_terminal(&mut terminal)?;
    let _ = worker.join();
    run_result?;
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
