//! Full-screen subject viewer (`v` on a PDF attachment in the projects
//! details): renders the converted markdown with scrolling.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{markdown, theme, widgets, wrap};
use crate::app::App;
use crate::input::Action;
use crate::state::Loadable;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(view) = &app.subject_view else {
        return;
    };
    // Full-screen overlay: wipe the underlying screen first so the viewer
    // is opaque instead of showing the tab behind it.
    frame.render_widget(Clear, area);
    let block = theme::pane(true).title(Span::styled(
        format!(
            " {} — subject (j/k scroll · d/u half page · g/G ends · Esc close) ",
            view.title
        ),
        theme::title(),
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match &view.content {
        Loadable::Idle | Loadable::Loading => {
            let mut lines = vec![widgets::loading_line(
                "fetching + converting subject…",
                app.tick,
            )];
            lines.push(Line::from(Span::styled(
                "  the PDF is downloaded once, converted locally, then cached",
                theme::muted(),
            )));
            frame.render_widget(Paragraph::new(lines), inner);
        }
        Loadable::Failed(message) => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("could not convert subject: {message}"),
                    theme::error(),
                ))),
                inner,
            );
        }
        Loadable::Ready(markdown) => {
            let lines = markdown::render(markdown);
            let lines = wrap::hanging(lines, inner.width);
            view.total_height.set(lines.len() as u16);
            view.view_height.set(inner.height);
            let visible = inner.height.max(1);
            let max_scroll = (lines.len() as u16).saturating_sub(visible);
            let scroll = view.scroll.min(max_scroll);
            frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
            if max_scroll > 0 {
                let gauge = Rect {
                    y: area.bottom().saturating_sub(1),
                    height: 1,
                    ..area
                };
                let at = (scroll as f32 / max_scroll as f32).clamp(0.0, 1.0);
                let filled = (at * (gauge.width as f32 - 18.0)) as u16;
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(format!("{:>3}% ", (at * 100.0) as u16), theme::muted()),
                        Span::styled("█".repeat(filled as usize), theme::muted()),
                        Span::styled(
                            "░".repeat(
                                ((gauge.width as f32 - 18.0) as usize)
                                    .saturating_sub(filled as usize),
                            ),
                            theme::muted(),
                        ),
                    ])),
                    gauge,
                );
            }
        }
    }
}

/// Keys inside the subject viewer; routed before anything else so the
/// overlay swallows global shortcuts and tab switches.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    let Some(view) = &mut app.subject_view else {
        return Action::Continue;
    };
    let height = view.view_height.get().max(1);
    let total = view.total_height.get();
    let max_scroll = total.saturating_sub(height);
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.subject_view = None,
        KeyCode::Char('j') | KeyCode::Down => {
            view.scroll = (view.scroll + 1).min(max_scroll);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            view.scroll = view.scroll.saturating_sub(1);
        }
        KeyCode::Char('d') | KeyCode::PageDown => {
            view.scroll = (view.scroll + height / 2).min(max_scroll);
        }
        KeyCode::Char('u') | KeyCode::PageUp => {
            view.scroll = view.scroll.saturating_sub(height / 2);
        }
        KeyCode::Char('g') | KeyCode::Home => view.scroll = 0,
        KeyCode::Char('G') | KeyCode::End => view.scroll = max_scroll,
        _ => {}
    }
    Action::Continue
}
