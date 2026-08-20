//! Content-stream walker: pages → positioned word spans, placed images and
//! stroked line segments (the table grids).
//!
//! Coordinates are device space with a **top-left** origin to keep the layout
//! thresholds from the original tooling: `y` grows downward, `x` rightward.

use std::collections::HashMap;

use pdf::content::{Op, TextDrawAdjusted};
use pdf::object::XObject;
use pdf::font::Font;
use pdf::object::{Resolve, Resources};
use pdf::primitive::Name;

/// One word (or kerned word group) with uniform formatting.
#[derive(Clone, Debug)]
pub struct Span {
    pub text: String,
    /// Base font name, subset prefix stripped (`LMRoman12-Bold`).
    pub font: String,
    pub size: f32,
    /// Fill color (`RRGGBB`).
    pub color: u32,
    pub x0: f32,
    pub x1: f32,
    /// Baseline position (top-left coords).
    pub y: f32,
}

/// A figure placement: where it was drawn and which object holds the data.
#[derive(Clone, Debug)]
pub struct PlacedImage {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub xref: pdf::object::Ref<XObject>,
}

/// A thin stroked segment (table ruling). Horizontal/vertical only.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub color: u32,
}

/// Everything one page contributes to the conversion.
#[derive(Default, Debug)]
pub struct PageData {
    pub spans: Vec<Span>,
    pub images: Vec<PlacedImage>,
    pub segments: Vec<Segment>,
}

/// Strip the `ABCDEF+` subset prefix from a base font name.
fn base_font_name(name: &str) -> String {
    match name.split_once('+') {
        Some((_, rest)) => rest.to_owned(),
        None => name.to_owned(),
    }
}

fn color_hex(c: &pdf::content::Color) -> u32 {
    let (r, g, b) = match c {
        pdf::content::Color::Gray(v) => (*v, *v, *v),
        pdf::content::Color::Rgb(rgb) => (rgb.red, rgb.green, rgb.blue),
        pdf::content::Color::Cmyk(c) => (
            (1.0 - c.cyan - c.key).clamp(0.0, 1.0),
            (1.0 - c.magenta - c.key).clamp(0.0, 1.0),
            (1.0 - c.yellow - c.key).clamp(0.0, 1.0),
        ),
        pdf::content::Color::Other(_) => (0.0, 0.0, 0.0),
    };
    ((r * 255.0).round() as u32) << 16
        | ((g * 255.0).round() as u32) << 8
        | (b * 255.0).round() as u32
}

struct FontInfo {
    base: String,
    to_unicode: Option<pdf::font::ToUnicodeMap>,
    widths: Option<pdf::font::Widths>,
}

fn decode(font: &FontInfo, bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if let Some(map) = &font.to_unicode
            && let Some(s) = map.get(b as u16)
        {
            out.push_str(s);
        } else if (32..127).contains(&b) {
            out.push(b as char);
        } else {
            out.push('\u{FFFD}');
        }
    }
    out
}

/// Advance width in text space (PDF 9.4.4).
fn text_width(widths: Option<&pdf::font::Widths>, bytes: &[u8], size: f32) -> f32 {
    let mut total = 0.0;
    for &b in bytes {
        let w = match widths {
            Some(ws) => ws.get(b as usize),
            None => 500.0,
        };
        total += w / 1000.0 * size;
    }
    total
}

fn font_table(resources: &Resources, resolve: &impl Resolve) -> HashMap<Name, FontInfo> {
    let mut table = HashMap::new();
    for (name, lazy) in &resources.fonts {
        let Ok(font) = lazy.load(resolve) else {
            continue;
        };
        let font: &Font = &font;
        let base = font
            .info()
            .and_then(|info| info.base_font.as_ref())
            .map(|n| base_font_name(n.as_str()))
            .unwrap_or_default();
        let to_unicode = font.to_unicode(resolve).and_then(|map| map.ok());
        let widths = font.widths(resolve).ok().flatten();
        table.insert(
            name.clone(),
            FontInfo {
                base,
                to_unicode,
                widths,
            },
        );
    }
    table
}

type Mat = [f32; 6];

fn mul(m: &Mat, n: &Mat) -> Mat {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn apply(m: &Mat, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// Extract one page's content. Never fails: broken pieces are skipped so a
/// single odd page cannot take the whole subject down.
pub fn page(page: &pdf::object::Page, resolve: &impl Resolve) -> PageData {
    let height = page.media_box.map(|b| b.top - b.bottom).unwrap_or(841.89);
    let mut data = PageData::default();
    let Some(contents) = &page.contents else {
        return data;
    };
    let Ok(ops) = contents.operations(resolve) else {
        return data;
    };
    let Some(resources) = page.resources.as_ref() else {
        return data;
    };
    let fonts = font_table(resources, resolve);
    walk(
        &ops,
        resources,
        &fonts,
        resolve,
        &mut data,
        height,
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    );
    data
}

#[allow(clippy::too_many_arguments)]
fn walk(
    ops: &[Op],
    resources: &Resources,
    fonts: &HashMap<Name, FontInfo>,
    resolve: &impl Resolve,
    data: &mut PageData,
    height: f32,
    ctm_in: Mat,
) {
    let mut ctm = ctm_in;
    let mut gstack: Vec<Mat> = Vec::new();
    let mut stroke_color = 0u32;

    let mut tm: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut tlm: Mat = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut leading = 0.0f32;
    let mut font: Option<(Name, f32)> = None;
    let mut fill = 0u32;
    let mut horiz = 1.0f32;
    let mut word_space = 0.0f32;
    let mut char_space = 0.0f32;
    let mut pen = (0.0f32, 0.0f32); // text space

    let mut cur: Option<Span> = None;
    macro_rules! flush {
        () => {
            if let Some(span) = cur.take() && !span.text.trim().is_empty() {
                data.spans.push(span);
            }
        };
    }

    let mut path: Vec<(f32, f32)> = Vec::new();

    for op in ops {
        match op {
            Op::Save => gstack.push(ctm),
            Op::Restore => {
                if let Some(saved) = gstack.pop() {
                    ctm = saved;
                }
            }
            Op::Transform { matrix } => {
                let m = [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f];
                ctm = mul(&m, &ctm);
            }
            Op::BeginText => {
                tm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                tlm = tm;
                pen = (0.0, 0.0);
            }
            Op::SetTextMatrix { matrix } => {
                tm = [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f];
                tlm = tm;
                pen = (0.0, 0.0);
                flush!();
            }
            Op::MoveTextPosition { translation } => {
                tlm = [
                    tlm[0],
                    tlm[1],
                    tlm[2],
                    tlm[3],
                    tlm[4] + translation.x,
                    tlm[5] + translation.y,
                ];
                tm = tlm;
                pen = (0.0, 0.0);
                flush!();
            }
            Op::TextNewline => {
                tlm = [
                    tlm[0], tlm[1], tlm[2], tlm[3], tlm[4], tlm[5] - leading,
                ];
                tm = tlm;
                pen = (0.0, 0.0);
                flush!();
            }
            Op::Leading { leading: l } => leading = *l,
            Op::TextScaling { horiz_scale } => horiz = *horiz_scale / 100.0,
            Op::WordSpacing { word_space: w } => word_space = *w,
            Op::CharSpacing { char_space: c } => char_space = *c,
            Op::TextFont { name, size } => {
                flush!();
                font = Some((name.clone(), *size));
            }
            Op::FillColor { color } => fill = color_hex(color),
            Op::StrokeColor { color } => stroke_color = color_hex(color),
            Op::TextDraw { text } => {
                draw(
                    text.as_bytes(),
                    fonts,
                    &font,
                    fill,
                    horiz,
                    word_space,
                    char_space,
                    &mut pen,
                    &ctm,
                    &tm,
                    height,
                    &mut cur,
                    data,
                );
            }
            Op::TextDrawAdjusted { array } => {
                for part in array {
                    match part {
                        TextDrawAdjusted::Text(text) => draw(
                            text.as_bytes(),
                            fonts,
                            &font,
                            fill,
                            horiz,
                            word_space,
                            char_space,
                            &mut pen,
                            &ctm,
                            &tm,
                            height,
                            &mut cur,
                            data,
                        ),
                        TextDrawAdjusted::Spacing(spacing) => {
                            if let Some((_, size)) = &font {
                                pen.0 -= spacing / 1000.0 * size * horiz;
                            }
                        }
                    }
                }
            }
            Op::XObject { name } => {
                flush!();
                invoke(name, resources, resolve, data, height, &ctm);
            }
            Op::MoveTo { p } => path = vec![(p.x, p.y)],
            Op::LineTo { p } => path.push((p.x, p.y)),
            Op::Rect { rect } => {
                let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
                path = vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
            }
            Op::Stroke | Op::FillAndStroke { .. } => {
                let device: Vec<(f32, f32)> =
                    path.iter().map(|&(x, y)| apply(&ctm, x, y)).collect();
                for pair in device.windows(2) {
                    let (x0, y0) = pair[0];
                    let (x1, y1) = pair[1];
                    if (x0 - x1).abs() < 0.7 || (y0 - y1).abs() < 0.7 {
                        data.segments.push(Segment {
                            x0: x0.min(x1),
                            y0: height - y0.max(y1),
                            x1: x0.max(x1),
                            y1: height - y0.min(y1),
                            color: stroke_color,
                        });
                    }
                }
                path.clear();
            }
            // `h` (Close) only closes the subpath — the path survives
            // until the painting operator (`S` above / `n` / `f` here).
            Op::EndPath | Op::Fill { .. } | Op::Clip { .. } => {
                path.clear();
            }
            _ => {}
        }
    }
    flush!();
}

fn invoke(
    name: &Name,
    resources: &Resources,
    resolve: &impl Resolve,
    data: &mut PageData,
    height: f32,
    ctm: &Mat,
) {
    let Some(xref) = resources.xobjects.get(name) else {
        return;
    };
    let Ok(xobject) = resolve.get(*xref) else {
        return;
    };
    match &*xobject {
        XObject::Image(image) => {
            // An image maps the unit square through the current CTM — the
            // `cm` before `Do` already carries the placement size.
            let _ = image;
            let corners = [
                apply(ctm, 0.0, 0.0),
                apply(ctm, 1.0, 0.0),
                apply(ctm, 0.0, 1.0),
                apply(ctm, 1.0, 1.0),
            ];
            let x0 = corners.iter().fold(f32::MAX, |a, c| a.min(c.0));
            let raw_min = corners.iter().fold(f32::MAX, |a, c| a.min(c.1));
            let x1 = corners.iter().fold(f32::MIN, |a, c| a.max(c.0));
            let raw_max = corners.iter().fold(f32::MIN, |a, c| a.max(c.1));
            data.images.push(PlacedImage {
                x0,
                y0: height - raw_max,
                y1: height - raw_min,
                x1,
                xref: *xref,
            });
        }
        XObject::Form(form) => {
            // pdfTeX wraps every figure in a Form transparency group;
            // recurse with the form's matrix and its own resources.
            let dict = form.dict();
            let m: Mat = match &dict.matrix {
                Some(matrix) => [
                    matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
                ],
                None => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            };
            let inner = mul(&m, ctm);
            let ops = match form.operations(resolve) {
                Ok(ops) => ops,
                Err(error) => {
                    if std::env::var("CLI42_DEBUG_FORM").is_ok() {
                        eprintln!("form ops failed: {error}");
                    }
                    return;
                }
            };
            if std::env::var("CLI42_DEBUG_FORM").is_ok() {
                eprintln!(
                    "form: {} ops, resources fonts={}",
                    ops.len(),
                    dict.resources.as_ref().map(|r| r.fonts.len()).unwrap_or(0)
                );
            }
            let empty = Resources::default();
            let form_resources: &Resources = dict
                .resources
                .as_deref()
                .unwrap_or(&empty);
            let form_fonts = font_table(form_resources, resolve);
            walk(
                &ops,
                form_resources,
                &form_fonts,
                resolve,
                data,
                height,
                inner,
            );
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    bytes: &[u8],
    fonts: &HashMap<Name, FontInfo>,
    font: &Option<(Name, f32)>,
    fill: u32,
    horiz: f32,
    word_space: f32,
    char_space: f32,
    pen: &mut (f32, f32),
    ctm: &Mat,
    tm: &Mat,
    height: f32,
    cur: &mut Option<Span>,
    data: &mut PageData,
) {
    let Some((name, size)) = font else { return };
    let Some(info) = fonts.get(name) else { return };

    let full = mul(ctm, tm);
    let (dev_x, dev_y) = apply(&full, pen.0, pen.1);
    let scale = (full[0] * full[3] - full[1] * full[2]).abs().sqrt();
    let eff_size = size * scale;

    let text = decode(info, bytes);
    let mut advance = text_width(info.widths.as_ref(), bytes, *size);
    advance += char_space * bytes.len() as f32
        + word_space * bytes.iter().filter(|&&b| b == b' ').count() as f32;
    advance *= horiz;

    let y = height - dev_y;
    let cont = match cur {
        Some(span) => {
            span.font == info.base
                && (span.size - eff_size).abs() < 0.05
                && span.color == fill
                && (span.y - y).abs() < 3.0
                // Kern overlaps (dev_x behind x1) continue; a forward jump
                // as wide as an inter-word space starts a new span.
                && dev_x > span.x0
                && dev_x - span.x1 < eff_size * 0.18
        }
        None => false,
    };
    if cont {
        let span = cur.as_mut().expect("continuation checked above");
        let gap = dev_x - span.x1;
        if gap > eff_size * 0.06 && !span.text.ends_with(' ') {
            span.text.push(' ');
        }
        span.text.push_str(&text);
        span.x1 = dev_x + advance * scale;
    } else {
        // A pen jump between show operations is an inter-word space the
        // typesetter emitted as a TJ adjustment rather than a glyph.
        if let Some(span) = cur.as_mut() {
            let gap = dev_x - span.x1;
            if gap > eff_size * 0.12 && !span.text.ends_with(' ') {
                span.text.push(' ');
            }
        }
        if let Some(span) = cur.take() && !span.text.trim().is_empty() {
            data.spans.push(span);
        }
        *cur = Some(Span {
            text,
            font: info.base.clone(),
            size: eff_size,
            color: fill,
            x0: dev_x,
            x1: dev_x + advance * scale,
            y,
        });
    }
    pen.0 += advance;
}
