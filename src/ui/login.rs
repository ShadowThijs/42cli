//! Login screen: username + password against Keycloak.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use tui_input::backend::crossterm::EventHandler;

use super::theme;
use super::widgets;
use crate::app::App;
use crate::input::Action;
use crate::state::Loadable;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    // 4 title + 3 username + 3 password + 3 status + 3 gaps = 16 rows.
    let centered = center(area, 46, 16);
    let chunks = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .spacing(1)
    .split(centered);

    let title = Line::from(vec![
        Span::styled(
            "4",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "2",
            Style::default()
                .fg(theme::BRIGHT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "cli",
            Style::default()
                .fg(theme::BRIGHT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  intra sign-in", theme::muted()),
    ]);
    frame.render_widget(
        Paragraph::new(title).alignment(ratatui::layout::Alignment::Center),
        chunks[0],
    );

    widgets::input_field(
        frame,
        chunks[1],
        " login",
        &app.login.username,
        app.login.focus == 0,
        false,
    );
    widgets::input_field(
        frame,
        chunks[2],
        " password",
        &app.login.password,
        app.login.focus == 1,
        true,
    );

    let status = match &app.login.state {
        Loadable::Loading => Some(widgets::loading_line("authenticating…", app.tick)),
        Loadable::Failed(message) => Some(
            Line::from(Span::styled(message.clone(), theme::error()))
                .alignment(ratatui::layout::Alignment::Center),
        ),
        _ => Some(
            Line::from(Span::styled(
                "Tab switch · Enter sign in · Ctrl+C quit",
                theme::muted(),
            ))
            .alignment(ratatui::layout::Alignment::Center),
        ),
    };
    if let Some(status) = status {
        frame.render_widget(status, chunks[3]);
    }
}

fn center(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    let horizontal = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(vertical[1]);
    horizontal[1]
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => return Action::Quit,
        KeyCode::Tab => {
            app.login.focus = 1 - app.login.focus;
        }
        KeyCode::Enter if app.login.focus == 0 => {
            app.login.focus = 1;
        }
        KeyCode::Enter => {
            let username = app.login.username.value().to_owned();
            let password = app.login.password.value().to_owned();
            if username.is_empty() || password.is_empty() {
                app.set_status("enter login and password");
                return Action::Continue;
            }
            app.login.state = Loadable::Loading;
            app.send(crate::bus::Command::Login { username, password });
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Action::Quit;
        }
        _ => {
            let input = if app.login.focus == 0 {
                &mut app.login.username
            } else {
                &mut app.login.password
            };
            input.handle_event(&crossterm::event::Event::Key(key));
        }
    }
    Action::Continue
}
