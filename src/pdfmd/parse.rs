//! Layout grammar of 42 subject PDFs: positioned spans → document blocks.
//!
//! The LaTeX class behind every subject is rigid, so the structure can be
//! read off a handful of signals (font size, family, color, indent):
//!
//! | construct          | signal                                              |
//! |--------------------|-----------------------------------------------------|
//! | chapter            | ~24.8pt bold, `Chapter {roman}`                     |
//! | section name       | ~24.8pt bold line following a chapter               |
//! | subsection         | ~17.2pt bold, optionally behind a purple `●`        |
//! | bullet / sub-bullet| `•`/`☛` / `◦` from MathSymbols/Dingbats             |
//! | good/bad practice  | `✓` green / `✗` red Dingbats + colored bold title   |
//! | note box           | ~10pt mono lines indented at x≈144                  |
//! | code block         | <10.5pt mono in light gray                          |
//! | table              | text inside a stroked line grid                     |
//! | figure             | large placed image, optional 10pt caption below     |

use super::extract::{PageData, PlacedImage, Segment, Span};

// ------------------------------------------------------- layout constants --
// A4 subject geometry (points). Body text starts at x=72; bullets at 89/115;
// note-box text at 144; page furniture beyond the y cutoffs.
const X_BODY: f32 = 72.0;
const X_BULLET: f32 = 89.0;
const X_SUB_BULLET: f32 = 115.0;
const X_INDENT: f32 = 88.0;
const X_NOTE: f32 = 130.0;
const Y_HEADER: f32 = 55.0;
const Y_FOOTER: f32 = 780.0;

const SIZE_CHAPTER: f32 = 22.0;
const SIZE_SECTION: f32 = 15.0;
const SIZE_BODY: f32 = 11.0;
const SIZE_CAPTION: f32 = 10.9;

const PURPLE: u32 = 0x800080;
const GREEN: u32 = 0x008000;
const RED: u32 = 0xB30000;

/// X-gap between spans that still reads as one flowing line.
const SPACE_GAP: f32 = 2.5;

// ------------------------------------------------------------- fragments --

#[derive(Clone, Debug, PartialEq)]
pub struct Frag {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub mono: bool,
}

impl Frag {
    fn plain(text: impl Into<String>) -> Self {
        Frag {
            text: text.into(),
            bold: false,
            italic: false,
            mono: false,
        }
    }

    pub(crate) fn style_eq(&self, other: &Frag) -> bool {
        self.bold == other.bold && self.italic == other.italic && self.mono == other.mono
    }
}

fn frag_of(span: &Span) -> Frag {
    Frag {
        text: span.text.clone(),
        bold: span.font.contains("Bold"),
        italic: span.font.contains("Italic") || span.font.contains("Oblique"),
        mono: span.font.contains("Mono"),
    }
}

/// x-sorted spans → styled fragments, spaces inserted at x-gaps.
fn spans_to_frags(spans: &[Span]) -> Vec<Frag> {
    let mut sorted: Vec<&Span> = spans.iter().collect();
    sorted.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());

    let mut frags: Vec<Frag> = Vec::new();
    let mut prev_x1 = None;
    for span in sorted {
        let frag = frag_of(span);
        match frags.last_mut() {
            Some(last) if last.style_eq(&frag) => {
                let gap = prev_x1.map_or(0.0, |x| span.x0 - x);
                if gap > SPACE_GAP && !last.text.ends_with(' ') {
                    last.text.push(' ');
                }
                last.text.push_str(&span.text);
            }
            Some(last)
                if prev_x1.is_some_and(|x| span.x0 - x <= SPACE_GAP)
                    && styles_italic_kin(last, &frag) =>
            {
                // The typesetter switches faces mid-word for digits and
                // punctuation (`ex0/` italic with an upright `0`); fuse so
                // the run renders as one italic word.
                last.italic = true;
                last.text.push_str(&span.text);
            }
            _ => {
                if prev_x1.is_some_and(|x| span.x0 - x > SPACE_GAP)
                    && frags.last().is_some_and(|f| !f.text.ends_with(' '))
                {
                    frags.push(Frag::plain(" "));
                }
                frags.push(frag);
            }
        }
        prev_x1 = Some(span.x1);
    }
    frags
}

/// Same emphasis except the italic bit, and the non-italic side carries no
/// letters — an upright digit/punctuation fragment inside an italic run.
fn styles_italic_kin(a: &Frag, b: &Frag) -> bool {
    let same_weight = a.bold == b.bold && a.mono == b.mono;
    let italic_split = a.italic != b.italic;
    let plain_side_letterless = if a.italic {
        !b.text.chars().any(char::is_alphabetic)
    } else {
        !a.text.chars().any(char::is_alphabetic)
    };
    same_weight && italic_split && plain_side_letterless
}

/// Words that keep their hyphen when a line break lands on them — TeX
/// hyphenates ordinary words at line ends, but an explicit compound
/// (`peer-evaluation`) breaks after its own hyphen, so these heads stay
/// joined while `How-`/`ever` fuse back into one word.
const COMPOUND_HEADS: [&str; 18] = [
    "peer", "copy", "sub", "non", "re", "co", "cross", "inter", "intra", "pre", "post", "self",
    "open", "semi", "multi", "well", "key", "left",
];

fn is_compound_head(head: &str) -> bool {
    let head = head.trim().to_lowercase();
    COMPOUND_HEADS.contains(&head.as_str())
}

/// Append `more` to `base` (both fragment lists), resolving end-of-line
/// hyphens when the join is a word continuation.
fn join_frags(base: &mut Vec<Frag>, more: Vec<Frag>) {
    // Keep whitespace-only frags: they are the separator a line's own
    // x-gaps produced; dropping them would glue words together.
    let more: Vec<Frag> = more.into_iter().filter(|f| !f.text.is_empty()).collect();
    if more.is_empty() {
        return;
    }
    let head = base
        .last()
        .and_then(|last| last.text.strip_suffix('-'))
        .map(|t| t.rsplit(' ').next().unwrap_or_default().to_owned());
    if let Some(head) = head {
        let continues = more[0]
            .text
            .chars()
            .next()
            .is_some_and(|c| c.is_lowercase());
        let same_style = base.last().is_some_and(|last| last.style_eq(&more[0]));
        if continues && same_style {
            let mut rest = more;
            let first = rest.remove(0);
            if let Some(last) = base.last_mut() {
                if !is_compound_head(&head) {
                    // Word hyphenation: "How-" + "ever" fuses back together.
                    last.text.pop();
                }
                // A compound head ("peer-") keeps its hyphen.
                last.text.push_str(&first.text);
            }
            base.extend(rest);
            return;
        }
    }
    if base.last().is_some_and(|f| !f.text.ends_with(' ')) {
        base.push(Frag::plain(" "));
    }
    base.extend(more);
}

// --------------------------------------------------------------- blocks --

#[derive(Clone, Debug)]
pub enum Block {
    Title(String),
    /// Italic summary line from the title page (already includes `Summary:`).
    Summary(String),
    Rule,
    Toc,
    TocEntry {
        level: u8,
        text: String,
    },
    Chapter(String),
    Section(String),
    Subsection(String),
    Paragraph(Vec<Frag>),
    Bullet {
        sub: bool,
        frags: Vec<Frag>,
    },
    /// One flowing paragraph inside `>`; optional `— Attribution`.
    Quote {
        frags: Vec<Frag>,
        attribution: Option<String>,
    },
    /// `#### Good practice:` / `#### Bad practice:` header. The body that
    /// follows arrives as its own `Quote` block.
    Practice {
        good: bool,
        title: String,
    },
    Code(Vec<String>),
    Table {
        rows: Vec<Vec<Vec<Frag>>>,
    },
    Image {
        name: String,
        title: String,
        xref: pdf::object::Ref<pdf::object::XObject>,
    },
}

// ---------------------------------------------------------------- lines --

/// One visual line: spans grouped by baseline, in reading order.
struct Line {
    spans: Vec<Span>,
    x0: f32,
    y: f32,
    size: f32,
}

impl Line {
    /// Plain text with x-gap-aware spacing (headings, captions, TOC).
    fn text(&self) -> String {
        self.frags()
            .iter()
            .map(|f| f.text.as_str())
            .collect::<Vec<_>>()
            .join("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn frags(&self) -> Vec<Frag> {
        spans_to_frags(&self.spans)
    }

    /// Fragments of the content spans only (markers/symbol glyphs dropped).
    fn content_frags(&self) -> Vec<Frag> {
        let content: Vec<Span> = self
            .spans
            .iter()
            .filter(|s| !is_symbol(s))
            .cloned()
            .collect();
        spans_to_frags(&content)
    }
}

/// Group a page's spans into y-sorted lines.
fn lines_of(page: &PageData) -> Vec<Line> {
    let mut spans: Vec<&Span> = page
        .spans
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .collect();
    spans.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap()
            .then(a.x0.partial_cmp(&b.x0).unwrap())
    });

    let mut lines: Vec<Line> = Vec::new();
    for span in spans {
        match lines.last_mut() {
            Some(line)
                if (line.y - span.y).abs() <= 4.0
                    && span.size <= line.size * 1.6
                    && line.size <= span.size * 1.6 =>
            {
                line.spans.push(span.clone());
                line.x0 = line.x0.min(span.x0);
                line.size = line.size.max(span.size);
            }
            _ => lines.push(Line {
                spans: vec![span.clone()],
                x0: span.x0,
                y: span.y,
                size: span.size,
            }),
        }
    }
    for line in &mut lines {
        line.spans.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());
    }
    lines
}

/// Page furniture cutoffs (running header, page number).
fn content_y(y: f32) -> bool {
    (Y_HEADER..=Y_FOOTER).contains(&y)
}

fn is_symbol(span: &Span) -> bool {
    span.font.contains("Dingbats") || span.font.contains("MathSymbols")
}

fn is_mono(span: &Span) -> bool {
    span.font.contains("Mono")
}

fn all_font(line: &Line, needle: &str) -> bool {
    !line.spans.is_empty() && line.spans.iter().all(|s| s.font.contains(needle))
}

fn light_color(color: u32) -> bool {
    let (r, g, b) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
    r >= 0xC0 && g >= 0xC0 && b >= 0xC0
}

fn near_black(color: u32) -> bool {
    let (r, g, b) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
    r < 0x60 && g < 0x60 && b < 0x60
}

fn near(color: u32, target: u32) -> bool {
    let d = |shift: u32| (((color >> shift) & 0xFF) as i32) - (((target >> shift) & 0xFF) as i32);
    d(16).abs() < 0x18 && d(8).abs() < 0x18 && d(0).abs() < 0x18
}

// ---------------------------------------------------------------- tables --

/// One stroked grid region: column x positions + row boundary y positions.
struct Grid {
    x0: f32,
    #[allow(dead_code)]
    x1: f32,
    y0: f32,
    y1: f32,
    columns: Vec<f32>,
    rows: Vec<f32>,
}

fn detect_grids(segments: &[Segment]) -> Vec<Grid> {
    // Only near-black ruling counts (the light-gray code-box borders do not).
    let black: Vec<&Segment> = segments.iter().filter(|s| near_black(s.color)).collect();

    let mut verticals: Vec<(f32, f32, f32)> = Vec::new(); // x, y0, y1
    let mut horizontals: Vec<f32> = Vec::new(); // y
    for seg in &black {
        if (seg.x1 - seg.x0).abs() < 1.0 && seg.y1 - seg.y0 > 8.0 {
            verticals.push((seg.x0, seg.y0, seg.y1));
        } else if (seg.y1 - seg.y0).abs() < 1.0 && seg.x1 - seg.x0 > 30.0 {
            horizontals.push(seg.y0);
        }
    }
    if verticals.len() < 4 {
        return Vec::new();
    }

    // Column positions: several verticals sharing an x.
    verticals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut columns: Vec<f32> = Vec::new();
    for (x, _, _) in &verticals {
        match columns.last_mut() {
            Some(last) if x - *last < 2.5 => *last = (*last + x) / 2.0,
            _ => columns.push(*x),
        }
    }
    let columns: Vec<f32> = columns
        .into_iter()
        .filter(|&x| {
            verticals
                .iter()
                .filter(|(vx, _, _)| (vx - x).abs() < 2.5)
                .count()
                >= 2
        })
        .collect();
    if columns.len() < 2 {
        return Vec::new();
    }

    // Row boundaries: the verticals' own endpoints (subjects rule their
    // tables booktabs-style — top, below header, bottom — while the row
    // separators live only in the chained verticals), plus any horizontal
    // rules inside the extent.
    let extent = (
        verticals
            .iter()
            .map(|(_, y0, _)| *y0)
            .fold(f32::MAX, f32::min),
        verticals
            .iter()
            .map(|(_, _, y1)| *y1)
            .fold(f32::MIN, f32::max),
    );
    let mut ys: Vec<f32> = verticals
        .iter()
        .flat_map(|(_, y0, y1)| [*y0, *y1])
        .chain(
            horizontals
                .iter()
                .copied()
                .filter(|y| *y > extent.0 - 3.0 && *y < extent.1 + 3.0),
        )
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut rows: Vec<f32> = Vec::new();
    for y in ys {
        match rows.last_mut() {
            Some(last) if y - *last < 2.5 => *last = (*last + y) / 2.0,
            _ => rows.push(y),
        }
    }
    if rows.len() < 2 {
        return Vec::new();
    }

    vec![Grid {
        x0: columns.first().copied().unwrap_or_default(),
        x1: columns.last().copied().unwrap_or_default(),
        y0: rows.first().copied().unwrap_or_default(),
        y1: rows.last().copied().unwrap_or_default(),
        columns,
        rows,
    }]
}

fn line_in_grid(line: &Line, grid: &Grid) -> bool {
    line.y > grid.y0 - 3.0 && line.y < grid.y1 + 3.0 && line.x0 >= grid.x0 - 6.0
}

/// Cell fragments per row: spans bucketed by column position. The last
/// ruling vertical closes the table, so cell left edges are the others.
fn parse_table(lines: &[&Line], grid: &Grid) -> Vec<Vec<Vec<Frag>>> {
    let edges: Vec<f32> = grid
        .columns
        .iter()
        .copied()
        .take(grid.columns.len().saturating_sub(1))
        .collect();
    if edges.is_empty() {
        return Vec::new();
    }
    let mut rows: Vec<Vec<Vec<Frag>>> = Vec::new();
    for window in grid.rows.windows(2) {
        let (top, bottom) = (window[0], window[1]);
        let mut cells: Vec<Vec<Span>> = vec![Vec::new(); edges.len()];
        for line in lines {
            if line.y <= top + 1.0 || line.y > bottom + 3.0 {
                continue;
            }
            for span in &line.spans {
                let column = edges
                    .iter()
                    .filter(|&&edge| span.x0 >= edge - 2.0)
                    .count()
                    .saturating_sub(1)
                    .min(edges.len() - 1);
                cells[column].push(span.clone());
            }
        }
        if cells.iter().any(|cell| !cell.is_empty()) {
            rows.push(cells.into_iter().map(|c| spans_to_frags(&c)).collect());
        }
    }
    rows
}

// -------------------------------------------------------------- parsing --

/// Figure file name slug: `Mandatory part` → `Mandatory_part`.
fn slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if ch.is_whitespace() && !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_owned()
}

fn content_image(image: &PlacedImage) -> bool {
    let w = image.x1 - image.x0;
    let h = image.y1 - image.y0;
    // Small squares are the note-box icons; everything else with real
    // extent (button strips are ~45pt tall) is content.
    (100.0..=560.0).contains(&w) && (30.0..=700.0).contains(&h)
}

/// Per-section figure numbering, mirroring the reference naming
/// (`Preamble.png`, `Mandatory_part.png`, `Mandatory_part2.png`, …).
struct FigureNamer {
    section: String,
    count: usize,
}

impl FigureNamer {
    fn next(&mut self, caption: Option<String>) -> (String, String) {
        if let Some(text) = caption
            && !text.trim().is_empty()
        {
            return (format!("{}.png", slug(&text)), text.trim().to_owned());
        }
        self.count += 1;
        let section = if self.section.is_empty() {
            "figure".to_owned()
        } else {
            self.section.clone()
        };
        let name = if self.count == 1 {
            format!("{}.png", slug(&section))
        } else {
            format!("{}{}.png", slug(&section), self.count)
        };
        let title = if self.count == 1 {
            section
        } else {
            format!("{section} {}", self.count)
        };
        (name, title)
    }
}

/// Parse every page into a flat block list.
pub fn document(pages: &[PageData]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let Some(first) = pages.first() else {
        return blocks;
    };
    title_page(first, &mut blocks);

    let mut rest = pages.iter().skip(1).peekable();
    let mut has_toc = false;
    if rest.peek().is_some_and(|page| {
        lines_of(page)
            .first()
            .is_some_and(|l| l.text().trim() == "Contents")
    }) {
        has_toc = true;
        toc_page(rest.next().expect("peeked"), &mut blocks);
    }
    if has_toc {
        blocks.push(Block::Rule);
    }

    let mut figures = FigureNamer {
        section: String::new(),
        count: 0,
    };
    for page in rest {
        content_page(page, &mut blocks, &mut figures);
    }
    blocks
}

fn title_page(page: &PageData, blocks: &mut Vec<Block>) {
    let mut emitted = false;
    for line in lines_of(page).into_iter().filter(|l| content_y(l.y)) {
        let trimmed = line.text().trim().to_owned();
        if trimmed.starts_with("Summary:") {
            blocks.push(Block::Summary(trimmed));
            emitted = true;
        } else if line.size >= SIZE_CHAPTER && !all_font(&line, "Italic") {
            // The project name; the tagline under it is ~20.7pt.
            blocks.push(Block::Title(trimmed));
            emitted = true;
        }
    }
    if emitted {
        blocks.push(Block::Rule);
    }
}

fn toc_page(page: &PageData, blocks: &mut Vec<Block>) {
    blocks.push(Block::Toc);
    for line in lines_of(page)
        .into_iter()
        .skip(1)
        .filter(|l| content_y(l.y))
    {
        // Keep only the name column: drop the trailing page number, the
        // dot leaders and the numbering column(s) at the left.
        let content: Vec<Span> = line
            .spans
            .iter()
            .filter(|span| {
                let text = span.text.trim();
                if text.is_empty() || text == "." {
                    return false;
                }
                if span.x0 > 480.0 && text.chars().all(|c| c.is_ascii_digit()) {
                    return false;
                }
                if span.x0 < 128.0 && is_toc_number(text) {
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        if content.is_empty() {
            continue;
        }
        // Sub-entries start their names right of the top-level column.
        let level = if content[0].x0 > 125.0 { 1 } else { 0 };
        let text = spans_to_frags(&content)
            .iter()
            .map(|f| f.text.as_str())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            blocks.push(Block::TocEntry { level, text });
        }
    }
}

/// `IV`, `III.1` — the roman / roman.numer numbering columns.
fn is_toc_number(text: &str) -> bool {
    text.chars()
        .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | '.' | '0'..='9'))
        && text
            .chars()
            .any(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C'))
}

/// What a content line is, for the main loop.
enum LineKind {
    NoteMarker,
    Chapter,
    SectionName,
    SubsectionBullet,
    SubsectionBold,
    Practice(bool),
    Bullet(bool),
    Note,
    Code,
    QuoteItalic,
    Indented,
    Caption,
    Body,
}

fn classify(line: &Line) -> LineKind {
    let first = &line.spans[0];
    let trimmed = line.text().trim().to_owned();

    // `➠` cyan marker introducing a one-line note.
    if first.font.contains("Dingbats") && trimmed.starts_with('➠') {
        return LineKind::NoteMarker;
    }

    if line.size >= SIZE_CHAPTER && all_font(line, "Bold") {
        return if trimmed.starts_with("Chapter ") {
            LineKind::Chapter
        } else {
            LineKind::SectionName
        };
    }
    if first.font.contains("Dingbats") && near(first.color, PURPLE) && line.size >= SIZE_SECTION {
        return LineKind::SubsectionBullet;
    }
    if first.font.contains("Dingbats")
        && (trimmed.contains('✓') || trimmed.contains('✗'))
        && (near(first.color, GREEN) || near(first.color, RED))
    {
        return LineKind::Practice(trimmed.contains('✓'));
    }
    if line.size >= SIZE_SECTION && line.size < SIZE_CHAPTER && all_font(line, "Bold") {
        return LineKind::SubsectionBold;
    }
    if is_symbol(first) && line.size < SIZE_SECTION {
        return match trimmed.chars().next().unwrap_or(' ') {
            '◦' => LineKind::Bullet(true),
            '•' | '☛' | '▸' | '‣' | '▪' => LineKind::Bullet(false),
            _ => LineKind::Body,
        };
    }
    if is_mono(first) && line.size < 10.5 && light_color(first.color) && line.x0 < X_NOTE {
        return LineKind::Code;
    }
    if is_mono(first) && line.size <= 10.9 && line.x0 >= X_NOTE {
        return LineKind::Note;
    }
    if !is_mono(first) && line.spans.iter().all(|s| s.size <= SIZE_CAPTION) {
        return LineKind::Caption;
    }
    if all_font(line, "Italic") && trimmed.starts_with(['"', '“', '‘']) && line.x0 > X_BODY + 5.0
    {
        return LineKind::QuoteItalic;
    }
    if line.x0 > X_INDENT && !is_symbol(first) {
        return LineKind::Indented;
    }
    LineKind::Body
}

/// y-sorted stream of content lines and figures so illustrations land
/// between the right paragraphs.
enum Event<'a> {
    Line(usize),
    Figure(&'a PlacedImage),
}

#[allow(clippy::too_many_lines)]
fn content_page(page: &PageData, blocks: &mut Vec<Block>, figures: &mut FigureNamer) {
    let lines = lines_of(page);
    let grids = detect_grids(&page.segments);

    let mut skip: Vec<usize> = Vec::new();
    for grid in &grids {
        let mut table_lines = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.y >= Y_HEADER && line.y <= Y_FOOTER && line_in_grid(line, grid) {
                skip.push(index);
                table_lines.push(line);
            }
        }
        let rows = parse_table(&table_lines, grid);
        if !rows.is_empty() {
            blocks.push(Block::Table { rows });
        }
    }

    let mut events: Vec<Event> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.y >= Y_HEADER && line.y <= Y_FOOTER && !skip.contains(&index) {
            events.push(Event::Line(index));
        }
    }
    for image in &page.images {
        if content_image(image) {
            events.push(Event::Figure(image));
        }
    }
    events.sort_by_key(|event| match event {
        Event::Line(index) => lines[*index].y as u32,
        Event::Figure(image) => image.y0 as u32,
    });

    let structural = |line: &Line| {
        matches!(
            classify(line),
            LineKind::Chapter
                | LineKind::SectionName
                | LineKind::SubsectionBullet
                | LineKind::SubsectionBold
                | LineKind::Practice(_)
                | LineKind::Bullet(_)
                | LineKind::Code
                | LineKind::Note
        )
    };

    let figures_below = |y_from: f32, y_to: f32| {
        events.iter().any(|event| match event {
            Event::Figure(image) => image.y0 > y_from + 1.0 && image.y0 < y_to - 1.0,
            Event::Line(_) => false,
        })
    };

    let mut index = 0;
    while index < events.len() {
        if let Event::Figure(image) = &events[index] {
            index += 1;
            let caption = lines.iter().enumerate().find(|(i, line)| {
                line.y >= image.y1 - 2.0
                    && line.y <= image.y1 + 26.0
                    && line.size <= SIZE_CAPTION
                    && !is_mono(&line.spans[0])
                    && line.x0 > image.x0 - 40.0
                    && line.x0 < image.x1 + 40.0
                    && !skip.contains(i)
            });
            let caption_text = caption.map(|(i, line)| {
                skip.push(i);
                line.text().trim().to_owned()
            });
            let (name, title) = figures.next(caption_text);
            blocks.push(Block::Image {
                name,
                title,
                xref: image.xref,
            });
            continue;
        }
        let Event::Line(line_index) = &events[index] else {
            unreachable!("figure handled above");
        };
        let line_index = *line_index;
        index += 1;
        if skip.contains(&line_index) {
            continue;
        }
        let line = &lines[line_index];
        match classify(line) {
            LineKind::NoteMarker => {
                // One italic line; wrapped continuations are indented and
                // become their own quote, like the reference conversions.
                let mut frags = line.content_frags();
                for frag in &mut frags {
                    frag.italic = true;
                    frag.bold = false;
                }
                if !frags.is_empty() {
                    blocks.push(Block::Paragraph(frags));
                }
            }
            LineKind::Chapter => {
                blocks.push(Block::Chapter(line.text().trim().to_owned()));
            }
            LineKind::SectionName => {
                let name = line.text().trim().to_owned();
                blocks.push(Block::Section(name.clone()));
                figures.section = name;
                figures.count = 0;
            }
            LineKind::SubsectionBullet | LineKind::SubsectionBold => {
                let text = if matches!(classify(line), LineKind::SubsectionBullet) {
                    String::new()
                } else {
                    line.text().trim().to_owned()
                };
                let text = if text.is_empty() {
                    line.content_frags()
                        .iter()
                        .map(|f| f.text.as_str())
                        .collect::<Vec<_>>()
                        .join("")
                        .trim()
                        .to_owned()
                } else {
                    text
                };
                blocks.push(Block::Subsection(text));
            }
            LineKind::Practice(good) => {
                let title = line.text().replace(['✓', '✗'], "").trim().to_owned();
                blocks.push(Block::Practice { good, title });
                let accept =
                    |l: &Line| l.x0 >= 79.0 && l.size < SIZE_SECTION && !is_symbol(&l.spans[0]);
                let body = collect_body(&mut index, &events, &lines, &skip, &structural, &accept);
                if !body.is_empty() {
                    blocks.push(Block::Quote {
                        frags: body,
                        attribution: None,
                    });
                }
            }
            LineKind::Bullet(sub) => {
                let mut frags = line.content_frags();
                let floor = if sub {
                    X_SUB_BULLET - 4.0
                } else {
                    X_BULLET - 4.0
                };
                let accept =
                    |l: &Line| l.x0 >= floor && l.size < SIZE_SECTION && !is_symbol(&l.spans[0]);
                let more = collect_body(&mut index, &events, &lines, &skip, &structural, &accept);
                join_frags(&mut frags, more);
                blocks.push(Block::Bullet { sub, frags });
            }
            LineKind::Code => {
                let mut code = vec![code_text(line)];
                while let Some(Event::Line(next)) = events.get(index) {
                    let next = *next;
                    if skip.contains(&next) {
                        index += 1;
                        continue;
                    }
                    if matches!(classify(&lines[next]), LineKind::Code) {
                        code.push(code_text(&lines[next]));
                        index += 1;
                    } else {
                        break;
                    }
                }
                blocks.push(Block::Code(code));
            }
            LineKind::Note => {
                // The box typesets its text in a mono face; semantically it
                // is note prose, so drop the code styling.
                let unmono = |frags: Vec<Frag>| -> Vec<Frag> {
                    frags
                        .into_iter()
                        .map(|mut frag| {
                            frag.mono = false;
                            frag
                        })
                        .collect()
                };
                let mut frags = unmono(line.frags());
                while let Some(Event::Line(next)) = events.get(index) {
                    let next = *next;
                    if skip.contains(&next) {
                        index += 1;
                        continue;
                    }
                    if matches!(classify(&lines[next]), LineKind::Note) {
                        let more = unmono(lines[next].frags());
                        join_frags(&mut frags, more);
                        index += 1;
                    } else {
                        break;
                    }
                }
                blocks.push(Block::Quote {
                    frags,
                    attribution: None,
                });
            }
            LineKind::QuoteItalic => {
                let mut frags = line.frags();
                let mut attribution = None;
                while let Some(Event::Line(next)) = events.get(index) {
                    let next = *next;
                    if skip.contains(&next) {
                        index += 1;
                        continue;
                    }
                    let candidate = &lines[next];
                    let text = candidate.text().trim().to_owned();
                    if (candidate.x0 - line.x0).abs() < 10.0 && text.starts_with('—') {
                        attribution = Some(text.trim_start_matches('—').trim().to_owned());
                        index += 1;
                        break;
                    }
                    let continues = (candidate.x0 - line.x0).abs() < 10.0
                        && candidate.size < SIZE_SECTION
                        && !is_symbol(&candidate.spans[0])
                        && !structural(candidate)
                        && !matches!(classify(candidate), LineKind::Bullet(_));
                    if continues {
                        let more = candidate.frags();
                        join_frags(&mut frags, more);
                        index += 1;
                    } else {
                        break;
                    }
                }
                blocks.push(Block::Quote { frags, attribution });
            }
            LineKind::Indented => {
                let frags = line.frags();
                match blocks.last_mut() {
                    Some(Block::Quote { frags: last, .. }) => join_frags(last, frags),
                    _ => blocks.push(Block::Quote {
                        frags,
                        attribution: None,
                    }),
                }
            }
            LineKind::Caption | LineKind::Body => {
                let mut frags = line.frags();
                while let Some(Event::Line(next)) = events.get(index) {
                    let next = *next;
                    if skip.contains(&next) {
                        index += 1;
                        continue;
                    }
                    let candidate = &lines[next];
                    let continues = (candidate.x0 - line.x0).abs() < 20.0
                        && candidate.size < SIZE_SECTION
                        && candidate.size >= SIZE_BODY - 1.5
                        && !is_symbol(&candidate.spans[0])
                        && !structural(candidate)
                        && !matches!(
                            classify(candidate),
                            LineKind::Bullet(_)
                                | LineKind::Note
                                | LineKind::Code
                                | LineKind::QuoteItalic
                                | LineKind::Indented
                        )
                        && !figures_below(line.y, candidate.y);
                    if continues {
                        let more = candidate.frags();
                        join_frags(&mut frags, more);
                        index += 1;
                    } else {
                        break;
                    }
                }
                blocks.push(Block::Paragraph(frags));
            }
        }
    }
}

fn code_text(line: &Line) -> String {
    line.spans
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("")
        .trim_end()
        .to_owned()
}

/// Gather body/continuation lines while `accept` holds and nothing
/// structural intervenes; advances the event index past consumed lines.
fn collect_body(
    index: &mut usize,
    events: &[Event],
    lines: &[Line],
    skip: &[usize],
    structural: &dyn Fn(&Line) -> bool,
    accept: &dyn Fn(&Line) -> bool,
) -> Vec<Frag> {
    let mut frags: Vec<Frag> = Vec::new();
    while let Some(Event::Line(next)) = events.get(*index) {
        let next = *next;
        if skip.contains(&next) {
            *index += 1;
            continue;
        }
        let line = &lines[next];
        if structural(line) || !accept(line) {
            break;
        }
        let more = line.frags();
        join_frags(&mut frags, more);
        *index += 1;
    }
    frags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(font: &str, x0: f32, x1: f32, text: &str) -> Span {
        Span {
            text: text.into(),
            font: font.into(),
            size: 12.0,
            color: 0,
            x0,
            x1,
            y: 282.0,
        }
    }

    fn fixture_pages(name: &str) -> Vec<PageData> {
        use crate::pdfmd::extract;
        let bytes = match std::fs::read(format!(
            "{}/docs/42pdf2md/{name}",
            env!("CARGO_MANIFEST_DIR")
        )) {
            Ok(bytes) => bytes,
            Err(_) => return Vec::new(),
        };
        let file = pdf::file::FileOptions::uncached()
            .load(bytes.as_slice().to_vec())
            .unwrap();
        let resolve = file.resolver();
        (0..file.num_pages())
            .map(|n| extract::page(&file.get_page(n).unwrap(), &resolve))
            .collect()
    }

    #[test]
    fn grid_detection_amazing() {
        let pages = fixture_pages("example/Amazing.pdf");
        if pages.is_empty() {
            eprintln!("fixture missing; skipping");
            return;
        }
        let page = &pages[7];
        eprintln!("segments: {}", page.segments.len());
        let grids = detect_grids(&page.segments);
        eprintln!("grids: {}", grids.len());
        for grid in &grids {
            eprintln!(
                "  grid y {:.0}..{:.0} cols {:?} rows {:?}",
                grid.y0, grid.y1, grid.columns, grid.rows
            );
        }
        for seg in page.segments.iter() {
            eprintln!(
                "  seg ({:.0},{:.0})-({:.0},{:.0}) {:06x} h={:.1} v={:.1}",
                seg.x0,
                seg.y0,
                seg.x1,
                seg.y1,
                seg.color,
                seg.y1 - seg.y0,
                seg.x1 - seg.x0
            );
        }
    }

    fn fixture_markdown(name: &str) -> String {
        let bytes = match std::fs::read(format!(
            "{}/docs/42pdf2md/{name}",
            env!("CARGO_MANIFEST_DIR")
        )) {
            Ok(bytes) => bytes,
            Err(_) => return String::new(),
        };
        crate::pdfmd::convert(&bytes).expect("convert").markdown
    }

    #[test]
    fn netpractice_grammar() {
        let md = fixture_markdown("NetPractice.pdf");
        if md.is_empty() {
            eprintln!("fixture missing; skipping");
            return;
        }
        std::fs::write("/tmp/netpractice-ours.md", &md).unwrap();

        let expect = [
            "# NetPractice",
            "*Summary: Discover the basics of networking.*",
            "# Contents",
            "7. Submission and peer-evaluation",
            "## Chapter I",
            "## Preamble",
            "![Preamble](Preamble.png \"Preamble\")",
            "### Context",
            "#### ✓ Good practice:",
            "#### ✗ Bad practice:",
            "> I ask AI: “How do I test a sorting function?”",
            "![Mandatory part](Mandatory_part.png \"Mandatory part\")",
            "![Mandatory part 2](Mandatory_part2.png \"Mandatory part 2\")",
            "> In this activity, the networks you will work with are simulated",
        ];
        for needle in expect {
            assert!(md.contains(needle), "missing [{needle}]\n---\n{md}");
        }

        // Hyphenated word-breaks fuse; compounds keep their hyphen.
        assert!(
            md.contains("understanding. Make peer"),
            "hyphen fuse:\n{md}"
        );
        assert!(
            md.contains("alternative perspectives"),
            "hyphen fuse 2:\n{md}"
        );
        assert!(
            md.contains("peer-evaluation, I can’t explain"),
            "compound:\n{md}"
        );

        // Words never glue at style boundaries or line breaks.
        for glued in [
            "configure**IP",
            "yourtechnical",
            "configureIP",
            "of**computer",
            "the**subnet",
            "a**router",
        ] {
            assert!(!md.contains(glued), "glued [{glued}]");
        }
    }

    #[test]
    fn amazing_grammar() {
        let md = fixture_markdown("example/Amazing.pdf");
        if md.is_empty() {
            eprintln!("fixture missing; skipping");
            return;
        }
        std::fs::write("/tmp/amazing-ours.md", &md).unwrap();

        let expect = [
            "# A-Maze-ing",
            "*Summary: Create your own maze generator and display its result!*",
            "### III.1 General Rules",
            "### IV.1 Summary",
            "### IV.3 Configuration file format",
            "| Key | Description | Example |",
            "| `WIDTH` | Maze width (number of cells) | `WIDTH=20` |",
            "| `PERFECT` | Is the maze perfect? | `PERFECT=True` |",
            "| Bit | Direction |",
            "| 0 (LSB) | North |",
            "```sh",
            "python3 a_maze_ing.py config.txt",
            "```",
            "![Terminal default rendering of the maze]",
            "![Output file example]",
            "> *“A labyrinth is not a place to be lost, but a path to be found.”*",
            "> -- Anonymous",
            "- **lint**: Execute the commands `flake8 .` and",
        ];
        for needle in expect {
            assert!(md.contains(needle), "missing [{needle}]");
        }
        // The exercise grid table the old tool mangled into random tables:
        // exactly two tables, both with ruling.
        let tables = md.matches("\n| ").count();
        assert!(tables >= 2, "expected both tables, found {tables}");
    }

    #[test]
    fn hyphen_fuses_across_lines() {
        let mut base = vec![Frag::plain(
            "Explaining your reasoning often reveals gaps in your un-",
        )];
        join_frags(
            &mut base,
            vec![Frag::plain("derstanding. Make peer learning a priority.")],
        );
        let text: String = base.iter().map(|f| f.text.as_str()).collect();
        eprintln!("joined: [{text}]");
        assert!(text.contains("understanding"), "got: {text}");

        let mut base = vec![Frag::plain("During peer-")];
        join_frags(&mut base, vec![Frag::plain("evaluation, I can’t explain")]);
        let text: String = base.iter().map(|f| f.text.as_str()).collect();
        eprintln!("compound: [{text}]");
        assert!(text.contains("peer-evaluation"), "got: {text}");
    }

    #[test]
    fn space_at_style_boundary() {
        let frags = spans_to_frags(&[
            span("LMRoman12-Bold", 72.0, 196.5, "computer networking"),
            span("LMRoman12-Regular", 196.5, 199.8, "."),
            span(
                "LMRoman12-Regular",
                204.8,
                361.8,
                "You will learn how to configure",
            ),
            span("LMRoman12-Bold", 365.1, 438.2, "IP addresses"),
            span("LMRoman12-Regular", 438.2, 523.3, ", connect devices"),
        ]);
        let text = frags.iter().map(|f| f.text.as_str()).collect::<String>();
        eprintln!("frags: {frags:?}");
        eprintln!("text: [{text}]");
        assert!(text.contains("configure IP addresses"), "got: {text}");
    }
}
