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
        handle_notification_key(app, key);
        return Action::Continue;
    }
    // The subject viewer is a full-screen overlay: it swallows everything.
    if app.subject_view.is_some() {
        ui::subject::handle_key(app, key);
        return Action::Continue;
    }

    // The clone prompt is a proper overlay too — it must swallow F-keys and
    // global shortcuts while destination / folder name are being typed.
    if app.projects.clone_prompt.is_some() {
        ui::projects::handle_clone_prompt_key(app, key);
        return Action::Continue;
    }
    // So is the post-clone "open in editor?" popup.
    if app.projects.editor_prompt.is_some() {
        ui::projects::handle_editor_prompt_key(app, key);
        return Action::Continue;
    }
    // User-project popup is also an overlay: it must swallow global
    // shortcuts like `q` (quit) while open — `q` should only close the
    // popup, not the whole TUI (otherwise `q` propagates and exits).
    if app.user.popup.is_some() {
        ui::user::handle_key(app, key);
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

    // So do Ctrl+Left / Ctrl+Right (keyboards without an F-row). `tui_input`
    // has no binding on Ctrl+arrows, so this is safe inside text fields too.
    if key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::SHIFT)
    {
        match key.code {
            KeyCode::Left => {
                app.enter_tab(app.tab.step(-1));
                return Action::Continue;
            }
            KeyCode::Right => {
                app.enter_tab(app.tab.step(1));
                return Action::Continue;
            }
            _ => {}
        }
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
                app.notifications_sel = 0;
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

/// Keys inside the notifications overlay: j/k walk the list, Enter follows
/// the selected notification's link, the event popup replaces the list.
fn handle_notification_key(app: &mut App, key: KeyEvent) {
    if app.event_popup.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') | KeyCode::Char('q') => {
                app.event_popup = None;
                app.notifications_open = false;
            }
            KeyCode::Char('s') => set_event_subscription(app, true),
            KeyCode::Char('u') => set_event_subscription(app, false),
            _ => {}
        }
        return;
    }

    let count = app
        .dash
        .notifications
        .data()
        .map(|payload| payload.items.len())
        .unwrap_or(0);
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.notifications_sel = app.notifications_sel.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.notifications_sel = (app.notifications_sel + 1).min(count.saturating_sub(1));
        }
        KeyCode::Enter => open_selected_notification(app),
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
            app.notifications_open = false;
        }
        _ => {}
    }
}

/// Follow the selected notification's `link`: events open the detail popup,
/// project links jump to the project in the projects tab.
fn open_selected_notification(app: &mut App) {
    let Some(link) = app
        .dash
        .notifications
        .data()
        .and_then(|payload| payload.items.get(app.notifications_sel))
        .and_then(|notification| notification.link.clone())
    else {
        return;
    };

    if let Some(id) = event_id_from_link(&link) {
        app.event_popup = Some(crate::state::EventPopup {
            event_id: id,
            event: crate::state::Loadable::Loading,
        });
        app.send(crate::bus::Command::LoadEvent { id });
        return;
    }

    if let Some(slug) = project_slug_from_link(&link) {
        app.notifications_open = false;
        app.event_popup = None;
        app.enter_tab(Tab::Projects);
        ui::projects::focus_project(app, &slug);
        return;
    }

    app.notifications_open = false;
    app.set_status(format!("no viewer for {link}"));
}

/// Fire the subscribe (`s`) / unsubscribe (`u`) action of the open event
/// popup, replaying the footer form of the scraped event page.
fn set_event_subscription(app: &mut App, subscribe: bool) {
    let action = app.event_popup.as_ref().and_then(|popup| {
        let crate::state::Loadable::Ready(event) = &popup.event else {
            return None;
        };
        let url = if subscribe {
            event.subscribe_url.clone()
        } else {
            event.unsubscribe_url.clone()
        }?;
        let csrf_token = event.csrf_token.clone()?;
        Some((popup.event_id, url, csrf_token))
    });
    let Some((id, url, csrf_token)) = action else {
        // No footer action on the page: past, full or closed event.
        app.set_status(if subscribe {
            "this event cannot be subscribed to"
        } else {
            "this event cannot be unsubscribed from"
        });
        return;
    };
    app.set_status(if subscribe {
        "subscribing…"
    } else {
        "unsubscribing…"
    });
    app.send(crate::bus::Command::SetEventSubscription {
        id,
        url,
        csrf_token,
        subscribe,
    });
}

/// `https://profile.intra.42.fr/events/43447` -> `43447`.
fn event_id_from_link(link: &str) -> Option<u32> {
    link.split("/events/")
        .nth(1)?
        .trim_end_matches('/')
        .parse()
        .ok()
}

/// `https://projects.intra.42.fr/projects/datomic/` -> `"datomic"`.
fn project_slug_from_link(link: &str) -> Option<String> {
    let slug = link.split("/projects/").nth(1)?.trim_end_matches('/');
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_owned())
    }
}

/// True when the current focus is a text field where typed characters must
/// not be interpreted as global shortcuts.
fn text_input_active(app: &App) -> bool {
    match app.tab {
        Tab::Search => true,
        Tab::Projects => app.projects.filter_focused,
        _ => false,
    }
}

fn refresh_current_tab(app: &mut App) {
    match app.tab {
        Tab::Dashboard => app.send(crate::bus::Command::LoadDashboard { fresh: true }),
        Tab::Projects => app.send(crate::bus::Command::LoadProjects { fresh: true }),
        Tab::Clusters => app.send(crate::bus::Command::LoadClusters { fresh: true }),
        Tab::Slots => app.send(crate::bus::Command::LoadSlotsOverview {
            anchor: app.slots.week_anchor,
        }),
        Tab::User => {
            // Without an opened profile there is nothing to reload — asking
            // the API for an empty login would just error every pane.
            if app.user.login.is_empty() {
                app.set_status("search a user first (F4)");
            } else {
                app.send(crate::bus::Command::LoadUser {
                    login: app.user.login.clone(),
                });
            }
        }
        Tab::Search => app.set_status("search refreshes as you type"),
    }
}
