//! Hanging-indent word wrap for the subject viewer. Wrapped continuation
//! lines are indented past their paragraph's own start so they visibly
//! belong to the line above, and words wider than the pane (long code
//! lines) are hard-split.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar as _;

/// Extra columns a continuation line gains over its paragraph's own
/// leading whitespace.
const HANG: usize = 2;

/// Wrap every line to `width` columns; lines that fit pass through
/// untouched (figure lines are exactly `width` wide, so they are immune).
pub fn hanging(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1) as usize;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if line_width(&line) <= width {
            out.push(line);
            continue;
        }
        out.extend(wrap_line(line, width));
    }
    out
}

fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .flat_map(|span| span.content.chars())
        .map(|c| c.width().unwrap_or(0))
        .sum()
}

fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

fn wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    let flat: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|span| span.content.chars().map(|c| (c, span.style)))
        .filter(|(c, _)| char_width(*c) > 0)
        .collect();
    let lead = flat.iter().take_while(|(c, _)| *c == ' ').count();
    let cont = (lead + HANG).min(width / 2).max(1).min(width);

    let mut w = Wrapper {
        out: Vec::new(),
        cur: Vec::new(),
        buf: String::new(),
        buf_style: Style::default(),
        cur_w: 0,
        width,
        cont,
        first: true,
    };

    let mut word: Vec<(char, Style)> = Vec::new();
    let mut gap: Vec<(char, Style)> = Vec::new();
    for (c, style) in flat {
        if c == ' ' {
            if word.is_empty() {
                gap.push((c, style));
            } else {
                w.word(std::mem::take(&mut word), std::mem::take(&mut gap));
                gap.push((c, style));
            }
        } else {
            word.push((c, style));
        }
    }
    w.word(word, gap);
    w.finish()
}

struct Wrapper {
    out: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,
    buf: String,
    buf_style: Style,
    cur_w: usize,
    width: usize,
    cont: usize,
    first: bool,
}

impl Wrapper {
    fn push(&mut self, c: char, style: Style) {
        if !self.buf.is_empty() && style != self.buf_style {
            self.flush();
        }
        self.buf.push(c);
        self.buf_style = style;
        self.cur_w += char_width(c);
    }

    fn flush(&mut self) {
        if !self.buf.is_empty() {
            self.cur
                .push(Span::styled(std::mem::take(&mut self.buf), self.buf_style));
        }
    }

    /// Close the current line and start a continuation with the hanging
    /// indent. A line that is still empty (or only the indent) is reused
    /// instead of emitting a blank.
    fn start_cont(&mut self) {
        self.flush();
        let untouched = self.cur_w == 0
            || (!self.first && self.cur_w == self.cont && self.cur.len() <= 1);
        if !untouched {
            self.out.push(Line::from(std::mem::take(&mut self.cur)));
            self.cur = vec![Span::raw(" ".repeat(self.cont))];
            self.cur_w = self.cont;
        }
        self.first = false;
    }

    /// Place one word (and the whitespace run before it) using greedy wrap.
    fn word(&mut self, word: Vec<(char, Style)>, gap: Vec<(char, Style)>) {
        let word_w: usize = word.iter().map(|(c, _)| char_width(*c)).sum();
        let gap_w: usize = gap.iter().map(|(c, _)| char_width(*c)).sum();
        if self.cur_w + gap_w + word_w <= self.width {
            for (c, s) in gap {
                self.push(c, s);
            }
            for (c, s) in word {
                self.push(c, s);
            }
        } else if self.cont + word_w <= self.width {
            // Wrap before the word; the whitespace at the break is dropped.
            self.start_cont();
            for (c, s) in word {
                self.push(c, s);
            }
        } else {
            // Word wider than a line: fill the remainder, then hard-split.
            for (c, s) in gap {
                self.push(c, s);
            }
            for (c, s) in word {
                if self.cur_w + char_width(c) > self.width {
                    self.start_cont();
                }
                self.push(c, s);
            }
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush();
        self.out.push(Line::from(self.cur));
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> Line<'static> {
        Line::from(Span::raw(text.to_owned()))
    }

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.clone()).collect::<String>())
            .collect()
    }

    #[test]
    fn fitting_lines_pass_through() {
        let lines = vec![plain("short line")];
        let out = hanging(lines, 40);
        assert_eq!(text(&out), ["short line"]);
    }

    #[test]
    fn continuation_gets_hanging_indent() {
        // Body starts at column 2; continuations at 2 + HANG.
        let out = hanging(vec![plain("  aaa bbb ccc")], 10);
        assert_eq!(text(&out), ["  aaa bbb", "    ccc"]);
    }

    #[test]
    fn overlong_words_hard_split() {
        let out = hanging(vec![plain("  aaaaaaaaaa")], 8);
        assert_eq!(text(&out), ["  aaaaaa", "    aaaa"]);
    }

    #[test]
    fn exact_width_lines_are_untouched() {
        let line = Line::from(vec![Span::raw("▀".repeat(10))]);
        let out = hanging(vec![line], 10);
        assert_eq!(text(&out), ["▀".repeat(10)]);
    }
}
