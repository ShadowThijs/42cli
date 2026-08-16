//! Global key routing: overlays first, then F-key tab switching, then
//! screen-specific handlers that live next to their rendering code.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Screen, Tab};
use crate::ui;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Quit,
}

pub fn handle_term_event(app: &mut App, event: Event) -> Action {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key),
        _ => Action::Continue,
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    // Overlays swallow everything.
    if app.help_open {
        app.help_open = false;
        return Action::Continue;
    }
    if app.notifications_open {
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n')) {
            app.notifications_open = false;
        }
        return Action::Continue;
    }

    if app.screen == Screen::Login {
        return ui::login::handle_key(app, key);
    }

    // Function keys switch tabs from anywhere — including while typing.
    if let KeyCode::F(index) = key.code
        && (1..=Tab::ORDER.len() as u8).contains(&index)
    {
        app.enter_tab(Tab::from_index(index as usize - 1));
        return Action::Continue;
    }

    // Global keys (never while typing in an input field).
    if !text_input_active(app) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('q'), _) => {
                return Action::Quit;
            }
            (KeyCode::Char('?'), _) => {
                app.help_open = true;
                return Action::Continue;
            }
            (KeyCode::Char('n'), _) => {
                app.notifications_open = true;
                return Action::Continue;
            }
            (KeyCode::Char('L'), _) => {
                app.send(crate::bus::Command::Logout);
                return Action::Continue;
            }
            (KeyCode::Char('r'), _) => {
                refresh_current_tab(app);
                return Action::Continue;
            }
            _ => {}
        }
    }

    match app.tab {
        Tab::Dashboard => ui::dashboard::handle_key(app, key),
        Tab::Projects => ui::projects::handle_key(app, key),
        Tab::Slots => ui::slots::handle_key(app, key),
        Tab::Search => ui::search::handle_key(app, key),
        Tab::User => ui::user::handle_key(app, key),
        Tab::Clusters => ui::clusters::handle_key(app, key),
    }
}

/// True when the current focus is a text field where typed characters must
/// not be interpreted as global shortcuts.
fn text_input_active(app: &App) -> bool {
    match app.tab {
        Tab::Search => true,
        Tab::Projects => app.projects.filter_focused,
        Tab::Slots => app.slots.focus == crate::state::SlotsFocus::Form,
        _ => false,
    }
}

fn refresh_current_tab(app: &mut App) {
    match app.tab {
        Tab::Dashboard => app.send(crate::bus::Command::LoadDashboard { fresh: true }),
        Tab::Projects => app.send(crate::bus::Command::LoadProjects { fresh: true }),
        Tab::Clusters => app.send(crate::bus::Command::LoadClusters { fresh: true }),
        Tab::Slots => app.send(crate::bus::Command::LoadSlotsOverview),
        Tab::User => app.send(crate::bus::Command::LoadUser {
            login: app.user.login.clone(),
        }),
        Tab::Search => app.set_status("search refreshes as you type"),
    }
}
