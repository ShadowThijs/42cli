//! Terminal lifecycle + the synchronous UI event loop.
//!
//! The loop polls crossterm with a short timeout, drains worker messages
//! (non-blocking) and re-renders. All HTTP runs on the worker thread, so
//! the UI can never hang on a request.

use std::io::Stdout;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;
use crate::bus::MsgStream;
use crate::input::{self, Action};
use crate::ui;

const TICK: Duration = Duration::from_millis(250);

pub type Term = Terminal<CrosstermBackend<Stdout>>;

pub fn setup_terminal() -> std::io::Result<Term> {
    let mut stdout = std::io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    Ok(terminal)
}

pub fn restore_terminal(terminal: &mut Term) -> std::io::Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
    )?;
    terminal.show_cursor()
}

/// Run until the user quits. Worker answers are applied between frames.
pub fn run(terminal: &mut Term, app: &mut App, messages: MsgStream) -> std::io::Result<()> {
    loop {
        // 1. Drain finished background work so rendering is current.
        loop {
            match messages.try_recv() {
                Ok(msg) => app.on_msg(msg),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        // 1b. Poll for background update sentinel (written by update thread).
        if app.update_available.is_none() && app.tick.is_multiple_of(4) {
            let path = crate::config::cache_dir().join("update_available");
            if let Ok(tag) = std::fs::read_to_string(&path) {
                let tag = tag.trim().to_owned();
                if !tag.is_empty() && tag != crate::update::VERSION {
                    app.update_available = Some(tag);
                    let _ = std::fs::remove_file(path);
                }
            }
        }

        // 2. Draw.
        terminal.draw(|frame| ui::draw(frame, app))?;

        // 3. Wait for input (bounded, so spinners and toasts stay animated).
        if crossterm::event::poll(TICK)? {
            let event = crossterm::event::read()?;
            if input::handle_term_event(app, event) == Action::Quit {
                return Ok(());
            }
        }

        // 4. An editor chosen from the post-clone popup ends the session —
        //    `main` exec()s it in the cloned folder once the TUI is torn
        //    down, so quitting the editor lands back in the shell.
        if app.pending_editor.is_some() {
            return Ok(());
        }
        app.tick = app.tick.wrapping_add(1);
    }
}
