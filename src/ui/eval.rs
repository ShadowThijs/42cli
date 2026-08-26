//! Full-screen evaluation form: answer the correction grid, pick the
//! rating flag, write the closing comment and submit — everything the
//! browser's `/scale_teams/{id}/edit` page does.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::theme;
use super::widgets;
use crate::api::models::{EvalForm, EvalQuestion, QuestionKind};
use crate::app::App;
use crate::bus::Command;
use crate::input::Action;
use crate::state::{AnswerInput, EvalView};
use crate::util;

/// Centered popup rect (`width`×`height`) over `area`.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
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
    .split(area);
    Rect {
        x: horizontal[1].x,
        y: vertical[1].y,
        width: area.width.min(horizontal[1].width),
        height: area.height.min(vertical[1].height),
    }
}

/// Text gauge for one slider: `█████░░░░░ 50/100`.
fn bar(value: i64, max: i64, width: usize) -> String {
    let ratio = if max <= 0 {
        0.0
    } else {
        (value as f64 / max as f64).clamp(0.0, 1.0)
    };
    let filled = (ratio * width as f64).round() as usize;
    format!(
        "{}{}",
        "█".repeat(filled.min(width)),
        "░".repeat(width - filled.min(width))
    )
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(view) = &app.eval_view else {
        return;
    };
    let block = theme::pane(false).title(Span::styled(" evaluation ", theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .spacing(1)
    .split(inner);

    match &view.form {
        crate::state::Loadable::Ready(form) => {
            draw_header(frame, view, form, rows[0]);
            let columns =
                Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                    .spacing(1)
                    .split(rows[1]);
            draw_questions(frame, view, form, columns[0]);
            draw_right(frame, app.tick, view, form, columns[1]);
        }
        crate::state::Loadable::Loading => {
            frame.render_widget(
                Paragraph::new(widgets::loading_line("loading evaluation form…", app.tick)),
                rows[1],
            );
        }
        crate::state::Loadable::Failed(message) => {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("error: {message}"),
                    theme::error(),
                ))),
                rows[1],
            );
        }
        _ => {}
    }
    draw_footer(frame, view, rows[2]);

    if view.confirming {
        draw_confirm(frame, view, area);
    }
}

fn draw_header(frame: &mut Frame, view: &EvalView, form: &EvalForm, area: Rect) {
    let title = format!(
        "{} — {}",
        form.project,
        if form.correcteds.is_empty() {
            "team".into()
        } else {
            form.correcteds.join(", ")
        }
    );
    let mut lines = vec![Line::from(Span::styled(title, theme::bright()))];
    if let Some(repo) = &form.repo {
        let width = area.width.saturating_sub(2) as usize;
        lines.push(Line::from(Span::styled(
            util::truncate_str(repo, width),
            theme::muted(),
        )));
    }
    let answered = view
        .answers
        .iter()
        .zip(form.questions().iter())
        .filter(|(answer, question)| !answer.submit_value(question).is_empty())
        .count();
    lines.push(Line::from(vec![Span::styled(
        format!("{}/{} answered", answered, view.answers.len()),
        theme::muted(),
    )]));
    frame.render_widget(Paragraph::new(lines), area);
}

/// Left pane: sections with one row per question, the selected question's
/// guidelines expanded underneath.
fn draw_questions(frame: &mut Frame, view: &EvalView, form: &EvalForm, area: Rect) {
    let block = theme::pane(false).title(Span::styled(" scale ", theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut question_rows: Vec<usize> = Vec::new();
    for section in &form.sections {
        lines.push(Line::from(Span::styled(
            section.title.clone(),
            Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
        )));
        for question in &section.questions {
            let index = question_rows.len();
            let selected = view.sel == index;
            question_rows.push(lines.len());
            lines.push(question_row(
                question,
                view.answers.get(index),
                selected,
                width,
            ));
            if selected && !question.guidelines.is_empty() {
                for mut line in util::wrap_lines(&question.guidelines, width, 10) {
                    line.spans.insert(0, Span::styled("  ", theme::muted()));
                    lines.push(line);
                }
            }
        }
    }

    // Keep the selected question's row on screen.
    let visible = inner.height as usize;
    let target = question_rows.get(view.sel).copied().unwrap_or(0);
    let scroll = target.saturating_sub(visible.saturating_sub(1));
    let shown: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(shown), inner);
}

/// One question row: name on the left, its answer control on the right.
fn question_row(
    question: &EvalQuestion,
    answer: Option<&AnswerInput>,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let marker = if selected { "▸ " } else { "  " };
    let name = util::truncate_str(&question.name, width.saturating_sub(24).max(10));
    let name_style = if selected {
        theme::bright()
    } else {
        theme::text()
    };
    let control = match (&question.kind, answer) {
        (QuestionKind::Boolean { .. }, Some(AnswerInput::Boolean(yes))) => boolean_spans(*yes),
        (QuestionKind::Choice { options }, Some(AnswerInput::Choice(index))) => {
            let label = options
                .get(*index)
                .map(|option| option.label.clone())
                .unwrap_or_default();
            vec![Span::styled(label, theme::bright())]
        }
        (QuestionKind::Range { max, .. }, Some(AnswerInput::Range(current))) => vec![
            Span::styled(bar(*current, *max, 10), Style::default().fg(theme::ACCENT)),
            Span::styled(format!(" {current}/{max}"), theme::muted()),
        ],
        _ => vec![Span::styled("—", theme::muted())],
    };
    let mut spans = vec![
        Span::styled(marker, theme::muted()),
        Span::styled(name, name_style),
    ];
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let control_width: usize = control.iter().map(|span| span.content.width()).sum();
    let pad = width.saturating_sub(used + control_width + 1);
    spans.push(Span::raw(" ".repeat(pad + 1)));
    spans.extend(control);
    Line::from(spans)
}

fn boolean_spans(yes: bool) -> Vec<Span<'static>> {
    let (yes_style, no_style) = if yes {
        (theme::good(), theme::muted())
    } else {
        (theme::muted(), theme::error())
    };
    vec![
        Span::styled("● Yes", yes_style),
        Span::styled("  ", theme::muted()),
        Span::styled("● No", no_style),
    ]
}

/// Right pane: the flag selector with its grading consequence, the
/// comment editor and the submit row.
fn draw_right(frame: &mut Frame, tick: u64, view: &EvalView, form: &EvalForm, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .spacing(1)
    .split(area);
    let question_count = view.answers.len();

    // -- flags ---------------------------------------------------------
    let flag_block =
        theme::pane(view.sel == question_count).title(Span::styled(" ratings ", theme::title()));
    let flag_inner = flag_block.inner(rows[0]);
    frame.render_widget(flag_block, rows[0]);
    let mut flag_lines: Vec<Line> = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    let width = flag_inner.width.saturating_sub(2) as usize;
    let mut used = 0usize;
    for (index, flag) in form.flags.iter().enumerate() {
        let selected = index == view.flag;
        let style = if selected {
            Style::default()
                .fg(if flag.positive {
                    theme::GOOD
                } else {
                    theme::ERR
                })
                .add_modifier(Modifier::BOLD)
        } else if flag.positive {
            theme::good()
        } else {
            theme::muted()
        };
        let chip = format!(" {} ", flag.name);
        let chip_width = chip.width();
        if used + chip_width > width && !current.is_empty() {
            flag_lines.push(Line::from(std::mem::take(&mut current)));
            used = 0;
        }
        current.push(Span::styled(chip, style));
        used += chip_width;
    }
    if !current.is_empty() {
        flag_lines.push(Line::from(current));
    }
    if let Some(flag) = form.flags.get(view.flag) {
        flag_lines.push(Line::from(Span::styled(
            flag_consequence(flag).unwrap_or("grades normally from your answers"),
            if flag.positive {
                theme::muted()
            } else {
                theme::error()
            },
        )));
    }
    frame.render_widget(Paragraph::new(flag_lines), flag_inner);

    // -- comment ---------------------------------------------------------
    let comment_block = theme::pane(view.sel == question_count + 1)
        .title(Span::styled(" comment ", theme::title()));
    let comment_inner = comment_block.inner(rows[1]);
    frame.render_widget(comment_block, rows[1]);
    draw_comment(frame, view, comment_inner);

    // -- submit ------------------------------------------------------------
    let submit_block =
        theme::pane(view.sel == question_count + 2).title(Span::styled(" submit ", theme::title()));
    let submit_inner = submit_block.inner(rows[2]);
    frame.render_widget(submit_block, rows[2]);
    let label = if view.submitting {
        format!("{} submitting…", widgets::spinner(tick))
    } else {
        "Enter / F — finish evaluation".to_owned()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, theme::warn()))),
        submit_inner,
    );
}

/// The closing comment: a window of lines around the cursor.
fn draw_comment(frame: &mut Frame, view: &EvalView, area: Rect) {
    let height = area.height as usize;
    let (row, col) = view.comment.cursor();
    let first = row.saturating_sub(height.saturating_sub(1));
    let mut lines: Vec<Line> = Vec::new();
    for index in first..(first + height).min(view.comment.line_count()) {
        let mut spans = vec![Span::styled(
            format!("{:>3} ", index + 1),
            Style::default().fg(theme::DIM),
        )];
        let line = view.comment.line(index);
        if view.editing_comment && index == row {
            let before: String = line.chars().take(col).collect();
            let at: String = line.chars().skip(col).take(1).collect();
            let after: String = line.chars().skip(col + 1).collect();
            spans.push(Span::styled(before, theme::text()));
            spans.push(Span::styled(
                if at.is_empty() { " ".into() } else { at },
                Style::default()
                    .fg(ratatui::style::Color::Black)
                    .bg(theme::ACCENT),
            ));
            spans.push(Span::styled(after, theme::text()));
        } else {
            spans.push(Span::styled(line.to_owned(), theme::text()));
        }
        lines.push(Line::from(spans));
    }
    let count = format!("{}/2048", view.comment.char_count());
    let mut footer = Line::from(Span::styled(
        if view.editing_comment {
            "typing — Esc done"
        } else {
            "Enter to type"
        },
        theme::muted(),
    ));
    footer.spans.push(Span::styled(
        format!("  {count}"),
        Style::default().fg(theme::DIM),
    ));
    frame.render_widget(Paragraph::new(lines), area);
    frame.render_widget(
        Paragraph::new(footer),
        Rect {
            y: area.bottom().saturating_sub(1),
            height: 1,
            ..area
        },
    );
}

fn draw_footer(frame: &mut Frame, view: &EvalView, area: Rect) {
    let hint = if view.editing_comment {
        "chars/Enter/Backspace type · Esc done"
    } else {
        "j/k move · ←/→ or y/n answer · ←/→ flag · Enter comment · F finish · Esc close"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme::muted()))),
        area,
    );
}

/// The final confirmation popup: submitting is irreversible.
fn draw_confirm(frame: &mut Frame, view: &EvalView, area: Rect) {
    let Some(form) = view.form.data() else {
        return;
    };
    let popup = centered(area, 60, 12);
    let block = theme::pane(true).title(Span::styled(" finish evaluation? ", theme::title()));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    let flag = form.flags.get(view.flag);
    let mut lines = vec![
        Line::from(Span::styled(
            format!("project  {}", form.project),
            theme::text(),
        )),
        Line::from(Span::styled(
            format!(
                "flag     {}",
                flag.map(|flag| flag.name.clone()).unwrap_or_default()
            ),
            if flag.is_some_and(|flag| flag.positive) {
                theme::good()
            } else {
                theme::error()
            },
        )),
        Line::from(Span::styled(
            format!(
                "comment  {} chars — {:?}",
                view.comment.char_count(),
                util::truncate_str(&view.comment.text(), 40)
            ),
            theme::text(),
        )),
        Line::from(""),
    ];
    if let Some(consequence) = flag.and_then(flag_consequence) {
        lines.push(Line::from(Span::styled(consequence, theme::error())));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "y submit · anything else cancels",
        theme::bright(),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// What the guidelines promise: negative flags end the grading with a 0,
/// cheating reports a -42.
pub fn flag_consequence(flag: &crate::api::models::EvalFlag) -> Option<&'static str> {
    if flag.positive {
        return None;
    }
    if flag.name.to_lowercase().contains("cheat") {
        Some("cheating reported — grading stops, final grade -42")
    } else {
        Some("grading stops here — final grade 0")
    }
}

// ---------------------------------------------------------- interaction ----

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    let Some(view) = &mut app.eval_view else {
        return Action::Continue;
    };
    if view.submitting {
        return Action::Continue;
    }
    if view.confirming {
        if key.code == KeyCode::Char('y') {
            submit(app);
        } else {
            view.confirming = false;
        }
        return Action::Continue;
    }
    if view.editing_comment {
        handle_comment_key(view, key);
        return Action::Continue;
    }

    let question_count = view.answers.len();
    let last_row = question_count + 2;
    match key.code {
        KeyCode::Esc => {
            app.eval_view = None;
        }
        KeyCode::Down | KeyCode::Char('j') => view.sel = (view.sel + 1).min(last_row),
        KeyCode::Up | KeyCode::Char('k') => view.sel = view.sel.saturating_sub(1),
        KeyCode::Char('F' | 'f') => try_confirm(app),
        KeyCode::Enter => match view.sel {
            index if index < question_count => toggle_question(app, index),
            index if index == question_count => cycle_flag(app, 1),
            index if index == question_count + 1 => view.editing_comment = true,
            _ => try_confirm(app),
        },
        KeyCode::Left | KeyCode::Char('h') => match view.sel {
            index if index < question_count => adjust_question(app, index, -1),
            index if index == question_count => cycle_flag(app, -1),
            _ => {}
        },
        KeyCode::Right | KeyCode::Char('l') => match view.sel {
            index if index < question_count => adjust_question(app, index, 1),
            index if index == question_count => cycle_flag(app, 1),
            _ => {}
        },
        KeyCode::Char('y') if view.sel < question_count => set_boolean(view, view.sel, true),
        KeyCode::Char('n') if view.sel < question_count => set_boolean(view, view.sel, false),
        _ => {}
    }
    Action::Continue
}

fn handle_comment_key(view: &mut EvalView, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => view.editing_comment = false,
        KeyCode::Enter => view.comment.newline(),
        KeyCode::Backspace => view.comment.backspace(),
        KeyCode::Left => view.comment.left(),
        KeyCode::Right => view.comment.right(),
        KeyCode::Up => view.comment.up(),
        KeyCode::Down => view.comment.down(),
        KeyCode::Home => view.comment.home(),
        KeyCode::End => view.comment.end(),
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            view.comment.insert(ch);
        }
        _ => {}
    }
}

/// Toggles flip booleans, choices advance, sliders step up.
fn toggle_question(app: &mut App, index: usize) {
    // Sliders delegate to the stepping path before any borrows are held.
    let is_range = app
        .eval_view
        .as_ref()
        .and_then(|view| view.form.data())
        .is_some_and(|form| {
            matches!(
                form.questions().get(index).map(|q| &q.kind),
                Some(QuestionKind::Range { .. })
            )
        });
    if is_range {
        adjust_question(app, index, 1);
        return;
    }
    let Some(view) = &mut app.eval_view else {
        return;
    };
    let Some(form) = view.form.data() else {
        return;
    };
    let questions = form.questions();
    let Some(question) = questions.get(index) else {
        return;
    };
    match &question.kind {
        QuestionKind::Boolean { .. } => {
            if let Some(AnswerInput::Boolean(yes)) = view.answers.get_mut(index) {
                *yes = !*yes;
            }
        }
        QuestionKind::Choice { options } => {
            if let Some(AnswerInput::Choice(current)) = view.answers.get_mut(index) {
                *current = (*current + 1) % options.len().max(1);
            }
        }
        QuestionKind::Range { .. } => unreachable!("handled above"),
    }
}

fn set_boolean(view: &mut EvalView, index: usize, yes: bool) {
    if let Some(AnswerInput::Boolean(current)) = view.answers.get_mut(index) {
        *current = yes;
    }
}

/// `←/→` on a question: booleans set no/yes, choices cycle, sliders move
/// one step inside their bounds and their skill group's headroom.
fn adjust_question(app: &mut App, index: usize, direction: i64) {
    let Some(view) = &mut app.eval_view else {
        return;
    };
    let Some(form) = view.form.data() else {
        return;
    };
    let questions = form.questions();
    let Some(question) = questions.get(index) else {
        return;
    };
    match &question.kind {
        QuestionKind::Boolean { .. } => {
            if let Some(AnswerInput::Boolean(yes)) = view.answers.get_mut(index) {
                *yes = direction > 0;
            }
        }
        QuestionKind::Choice { options } => {
            let Some(AnswerInput::Choice(current)) = view.answers.get_mut(index) else {
                return;
            };
            if options.is_empty() {
                return;
            }
            let next = (*current as i64 + direction).rem_euclid(options.len() as i64);
            *current = next as usize;
        }
        QuestionKind::Range { .. } => {
            let target = slider_target(form, &view.answers, index, direction);
            if let Some(AnswerInput::Range(value)) = view.answers.get_mut(index) {
                *value = target;
            }
        }
    }
}

/// Move slider `index` by `direction` steps, clamped to its own range
/// and to what remains of its skill group's cap (standard ≤ 100,
/// bonus ≤ 25) after the group's other sliders.
pub fn slider_target(
    form: &EvalForm,
    answers: &[AnswerInput],
    index: usize,
    direction: i64,
) -> i64 {
    let questions = form.questions();
    let Some(QuestionKind::Range {
        min,
        max,
        step,
        skill,
        kind,
        ..
    }) = questions.get(index).map(|question| &question.kind)
    else {
        return 0;
    };
    let Some(AnswerInput::Range(current)) = answers.get(index) else {
        return 0;
    };
    let candidate = current + direction * step;
    let mut ceiling = *max;
    if let (Some(skill), Some(kind)) = (skill, kind) {
        let group_cap: i64 = if kind == "bonus" { 25 } else { 100 };
        let others: i64 = questions
            .iter()
            .enumerate()
            .filter(|(other, question)| {
                other != &index
                    && matches!(&question.kind, QuestionKind::Range { skill: Some(s), kind: Some(k), .. } if s == skill && k == kind)
            })
            .filter_map(|(other, _)| match answers.get(other) {
                Some(AnswerInput::Range(value)) => Some(*value),
                _ => None,
            })
            .sum();
        ceiling = ceiling.min(group_cap - others);
    }
    candidate.clamp(*min, ceiling.max(*min))
}

fn cycle_flag(app: &mut App, direction: i64) {
    let Some(view) = &mut app.eval_view else {
        return;
    };
    let Some(form) = view.form.data() else {
        return;
    };
    if form.flags.is_empty() {
        return;
    }
    let next = (view.flag as i64 + direction).rem_euclid(form.flags.len() as i64);
    view.flag = next as usize;
}

fn try_confirm(app: &mut App) {
    let Some(view) = &mut app.eval_view else {
        return;
    };
    if view.form.data().is_none() || view.submitting {
        return;
    }
    if view.comment.text().trim().is_empty() {
        app.set_status("write a closing comment first");
        return;
    }
    view.confirming = true;
}

fn submit(app: &mut App) {
    let Some(view) = &mut app.eval_view else {
        return;
    };
    let Some(form) = view.form.data() else {
        return;
    };
    let submission = view.submission(form);
    view.confirming = false;
    view.submitting = true;
    app.send(Command::SubmitEval(submission));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{EvalAnswer, EvalFlag, EvalQuestion, EvalSection};

    fn slider_form() -> EvalForm {
        // Two standard sliders of one skill (cap 100 together) plus one
        // bonus slider (cap 25), in the shape the Scales module renders.
        let question = |id: u64, skill: Option<&str>, kind: Option<&str>, max: i64| EvalQuestion {
            question_id: id,
            name: format!("q{id}"),
            guidelines: String::new(),
            answer_id: None,
            kind: QuestionKind::Range {
                min: 0,
                max,
                step: 5,
                value: 0,
                skill: skill.map(str::to_owned),
                kind: kind.map(str::to_owned),
            },
        };
        EvalForm {
            action: "/a".into(),
            authenticity_token: "t".into(),
            project: "p".into(),
            correcteds: vec!["student".into()],
            repo: None,
            sections: vec![EvalSection {
                title: "s".into(),
                questions: vec![
                    question(1, Some("7"), Some("standard"), 100),
                    question(2, Some("7"), Some("standard"), 100),
                    question(3, Some("7"), Some("bonus"), 25),
                ],
            }],
            flags: vec![
                EvalFlag {
                    id: 1,
                    name: "Ok".into(),
                    positive: true,
                },
                EvalFlag {
                    id: 7,
                    name: "Cheat".into(),
                    positive: false,
                },
                EvalFlag {
                    id: 8,
                    name: "Crash".into(),
                    positive: false,
                },
            ],
            default_flag: Some(1),
        }
    }

    fn answers(values: &[i64]) -> Vec<AnswerInput> {
        values.iter().map(|v| AnswerInput::Range(*v)).collect()
    }

    #[test]
    fn sliders_share_the_skill_cap() {
        let form = slider_form();
        // First slider at 80 leaves 20 of headroom for the second: four
        // steps (4 × 5) land exactly on it.
        let state = answers(&[80, 0, 0]);
        assert_eq!(slider_target(&form, &state, 1, 4), 20);
        // Stepping past the headroom clamps to what remains.
        let state = answers(&[80, 15, 0]);
        assert_eq!(slider_target(&form, &state, 1, 10), 20);
        // Bonus sliders draw from their own 25 cap, not standard's 100.
        let state = answers(&[100, 0, 10]);
        assert_eq!(slider_target(&form, &state, 2, 10), 25);
        // And never exceed their own max even with headroom left.
        let state = answers(&[0, 0, 20]);
        assert_eq!(slider_target(&form, &state, 2, 5), 25);
        // Without a skill group the own bounds rule.
        let mut lone = slider_form();
        lone.sections[0].questions[0].kind = QuestionKind::Range {
            min: 0,
            max: 30,
            step: 5,
            value: 0,
            skill: None,
            kind: None,
        };
        let state = answers(&[25, 0, 0]);
        assert_eq!(slider_target(&lone, &state, 0, 5), 30);
    }

    #[test]
    fn flags_state_their_consequence() {
        let ok = EvalFlag {
            id: 1,
            name: "Ok".into(),
            positive: true,
        };
        let cheat = EvalFlag {
            id: 7,
            name: "Cheat".into(),
            positive: false,
        };
        let crash = EvalFlag {
            id: 8,
            name: "Crash".into(),
            positive: false,
        };
        assert_eq!(flag_consequence(&ok), None);
        assert_eq!(
            flag_consequence(&cheat),
            Some("cheating reported — grading stops, final grade -42")
        );
        assert_eq!(
            flag_consequence(&crash),
            Some("grading stops here — final grade 0")
        );
    }

    #[test]
    fn view_state_builds_the_submission() {
        let mut form = slider_form();
        form.sections[0].questions[0].kind = QuestionKind::Boolean { yes: false };
        form.sections[0].questions[1].kind = QuestionKind::Choice {
            options: vec![
                crate::api::models::EvalChoice {
                    value: "a".into(),
                    label: "A".into(),
                    selected: false,
                },
                crate::api::models::EvalChoice {
                    value: "b".into(),
                    label: "B".into(),
                    selected: true,
                },
            ],
        };
        let mut view = EvalView::open(9_720_568);
        view.apply_form(form);
        // Flip the boolean to yes, leave the choice at its default, raise
        // the bonus slider, pick the cheat flag, write a comment.
        view.answers[0] = AnswerInput::Boolean(true);
        view.answers[2] = AnswerInput::Range(15);
        view.flag = 1; // Cheat
        view.comment.insert('g');
        view.comment.insert('g');
        let form = view.form.data().unwrap();
        let submission = view.submission(form);
        assert_eq!(submission.flag_id, 7);
        assert_eq!(submission.comment, "gg");
        assert_eq!(
            submission
                .answers
                .iter()
                .map(|EvalAnswer { value, .. }| value.clone())
                .collect::<Vec<_>>(),
            vec!["1".to_owned(), "b".to_owned(), "15".to_owned()]
        );
    }
}
