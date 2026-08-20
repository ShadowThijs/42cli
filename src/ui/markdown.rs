//! Render the markdown our subject converter produces into styled terminal
//! lines: headings, bullets, quotes, fenced code, tables and figure
//! placeholders, with `**bold**` / `*italic*` / `` `code` `` inline styles.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme;

/// Markdown text → styled lines. Paragraphs are already reflowed by the
/// converter, so no wrapping happens here — long lines are clipped.
pub fn render(markdown: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code = false;

    for raw in markdown.lines() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();

        if trimmed.starts_with("```") {
            lines.push(Line::from(Span::styled("  ┄┄┄┄┄", theme::muted())));
            in_code = !in_code;
            continue;
        }
        if in_code {
            lines.push(Line::from(Span::styled(
                format!("  {raw}"),
                Style::default().fg(theme::MAGENTA),
            )));
            continue;
        }

        // Figure references: not rendered in the TUI — terminal image
        // protocols don't survive a scrolling document. The PNGs land in
        // the cache dir next to the markdown for external viewing.
        if trimmed.starts_with("![") {
            continue;
        }

        let (level, rest) = if let Some(rest) = trimmed.strip_prefix("#### ") {
            (4, rest)
        } else if let Some(rest) = trimmed.strip_prefix("### ") {
            (3, rest)
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            (2, rest)
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            (1, rest)
        } else {
            (0, trimmed)
        };
        if level > 0 {
            // Practice headers carry their ✓/✗ verdict in the text.
            let style = if rest.starts_with('✓') {
                Style::default()
                    .fg(theme::GOOD)
                    .add_modifier(Modifier::BOLD)
            } else if rest.starts_with('✗') {
                Style::default().fg(theme::ERR).add_modifier(Modifier::BOLD)
            } else if level <= 2 {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::bright()
            };
            lines.push(Line::from(Span::styled(format!("  {rest}"), style)));
            continue;
        }

        if trimmed == "---" {
            lines.push(Line::from(Span::styled(
                "  ─────────────────────────────────────────",
                theme::muted(),
            )));
            continue;
        }

        if trimmed.starts_with("| ") && trimmed.ends_with('|') {
            let style = if trimmed.contains("---") {
                theme::muted()
            } else {
                theme::text()
            };
            lines.push(Line::from(Span::styled(format!("  {trimmed}"), style)));
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let pad = " ".repeat(indent.min(8));
            let mut spans = vec![Span::styled(format!("{pad}  • "), theme::muted())];
            spans.extend(inline(rest));
            lines.push(Line::from(spans));
            continue;
        }

        if let Some(quote) = trimmed.strip_prefix("> ") {
            let mut spans = vec![Span::styled("  │ ", theme::muted())];
            spans.extend(inline(quote));
            lines.push(Line::from(spans));
            continue;
        }
        if trimmed == ">" {
            lines.push(Line::from(Span::styled("  │", theme::muted())));
            continue;
        }

        let mut spans = vec![Span::styled("  ", Style::default())];
        spans.extend(inline(trimmed));
        lines.push(Line::from(spans));
    }
    lines
}

/// Inline markdown (`**b**`, `*i*`, `` `c` ``) → styled spans.
fn inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut at = 0;

    fn flush(plain: &mut String, spans: &mut Vec<Span<'static>>) {
        if !plain.is_empty() {
            spans.push(Span::styled(std::mem::take(plain), theme::text()));
        }
    }

    while at < text.len() {
        let rest = &text[at..];
        let marker = if rest.starts_with("**") {
            "**"
        } else if rest.starts_with('*') {
            "*"
        } else if rest.starts_with('`') {
            "`"
        } else {
            let ch = rest.chars().next().expect("non-empty slice");
            plain.push(ch);
            at += ch.len_utf8();
            continue;
        };
        let content_start = at + marker.len();
        if let Some(close) = text[content_start..].find(marker) {
            flush(&mut plain, &mut spans);
            let inner = &text[content_start..content_start + close];
            let style = match marker {
                "**" => theme::bright(),
                "*" => Style::default().add_modifier(Modifier::ITALIC),
                _ => Style::default().fg(theme::MAGENTA),
            };
            spans.push(Span::styled(inner.to_owned(), style));
            at = content_start + close + marker.len();
        } else {
            plain.push_str(marker);
            at += marker.len();
        }
    }
    flush(&mut plain, &mut spans);
    spans
}
