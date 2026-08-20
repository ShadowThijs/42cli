//! Blocks → markdown, styled after the hand-checked reference conversions
//! (`docs/42pdf2md/tests/NetPractice-correct.md`).

use super::parse::{Block, Frag};

/// A figure reference left for `convert` to materialize.
pub struct ImageSlot {
    pub name: String,
    pub xref: pdf::object::Ref<pdf::object::XObject>,
}

/// Render the block list. Returns the markdown plus the referenced image
/// slots in order of appearance.
pub fn markdown(blocks: &[Block]) -> (String, Vec<ImageSlot>) {
    let mut out = String::new();
    let mut refs = Vec::new();

    let mut toc_counter = 0;
    let mut toc_sub = 0;
    let mut prev_listy = false;

    for block in blocks {
        let rendered = match block {
            Block::Title(text) => Some(format!("# {text}")),
            Block::Summary(text) => Some(format!("*{text}*")),
            Block::Rule => Some("---".to_owned()),
            Block::Toc => Some("# Contents".to_owned()),
            Block::TocEntry { level, text } => {
                if *level == 0 {
                    toc_counter += 1;
                    toc_sub = 0;
                    Some(format!("{toc_counter}. {text}"))
                } else {
                    toc_sub += 1;
                    Some(format!("    {toc_sub}. {text}"))
                }
            }
            Block::Chapter(text) | Block::Section(text) => Some(format!("## {text}")),
            Block::Subsection(text) => Some(format!("### {text}")),
            Block::Paragraph(frags) => {
                let text = inline(frags);
                (!text.trim().is_empty()).then_some(text)
            }
            Block::Bullet { sub, frags } => {
                let text = inline(frags);
                let pad = if *sub { "    " } else { "" };
                Some(format!("{pad}- {text}"))
            }
            Block::Quote { frags, attribution } => {
                let mut text = inline(frags);
                if !text.starts_with('>') {
                    text = format!("> {text}");
                }
                if let Some(who) = attribution {
                    text.push_str("\n>\n> -- ");
                    text.push_str(who);
                }
                Some(text)
            }
            Block::Practice { good, title } => {
                let icon = if *good { "✓ " } else { "✗ " };
                Some(format!("#### {icon}{title}"))
            }
            Block::Code(lines) => Some(format!("```sh\n{}\n```", lines.join("\n"))),
            Block::Table { rows } => Some(table(rows)),
            Block::Image { name, title, xref } => {
                refs.push(ImageSlot {
                    name: name.clone(),
                    xref: *xref,
                });
                Some(format!("![{title}]({name} \"{title}\")"))
            }
        };
        let Some(text) = rendered else { continue };

        // List items stack on consecutive lines; everything else breathes.
        let listy = matches!(block, Block::Bullet { .. } | Block::TocEntry { .. });
        if !out.is_empty() {
            if listy && prev_listy {
                out.push('\n');
            } else {
                out.push_str("\n\n");
            }
        }
        out.push_str(&text);
        prev_listy = listy;
    }

    (out, refs)
}

/// Fragments → inline markdown (`**bold**`, `*italic*`, `` `code` ``).
fn inline(frags: &[Frag]) -> String {
    // Fuse same-style neighbours first so one emphasis run gets one marker
    // pair instead of `*this*` `*that*`.
    let mut space_pending: Option<bool> = None;
    let mut fused: Vec<Frag> = Vec::new();
    for frag in frags {
        // A whitespace-only frag bridges same-style neighbours so one
        // emphasis run keeps a single marker pair.
        if frag.text.trim().is_empty() {
            if let Some(pending) = &mut space_pending {
                *pending = true;
            } else {
                space_pending = Some(true);
            }
            continue;
        }
        match (fused.last_mut(), space_pending) {
            (Some(last), Some(true)) if last.style_eq(frag) => {
                last.text.push(' ');
                last.text.push_str(&frag.text);
            }
            _ => {
                if space_pending == Some(true) {
                    fused.push(Frag {
                        text: " ".into(),
                        ..frag.clone()
                    });
                }
                fused.push(frag.clone());
            }
        }
        space_pending = None;
    }

    let mut out = String::new();
    let last = fused.len().saturating_sub(1);
    for (index, frag) in fused.iter().enumerate() {
        let text = frag.text.trim();
        if text.is_empty() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }
        let lead_space = frag.text.starts_with(char::is_whitespace);
        if lead_space && !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }

        match (frag.bold, frag.italic, frag.mono) {
            (true, true, _) => out.push_str(&format!("***{text}***")),
            (true, _, false) => out.push_str(&format!("**{text}**")),
            (_, true, false) => out.push_str(&format!("*{text}*")),
            (_, _, true) => out.push_str(&format!("`{text}`")),
            (false, false, false) => out.push_str(text),
        }
        // Keep the separator a merged trailing space carried over.
        if index != last && frag.text.ends_with(char::is_whitespace) {
            out.push(' ');
        }
    }
    out.trim().to_owned()
}

fn table(rows: &[Vec<Vec<Frag>>]) -> String {
    let Some(first) = rows.first() else {
        return String::new();
    };
    let columns = first.len();
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let cells: Vec<String> = (0..columns)
            .map(|column| row.get(column).map(|cell| inline(cell)).unwrap_or_default())
            .collect();
        lines.push(format!("| {} |", cells.join(" | ")));
        if index == 0 {
            lines.push(format!("| {} |", vec!["---"; columns].join(" | ")));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdfmd::parse::Frag;

    #[test]
    fn inline_keeps_boundary_spaces() {
        let frags = vec![
            Frag {
                text: "You will learn how to configure".into(),
                bold: false,
                italic: false,
                mono: false,
            },
            Frag {
                text: " ".into(),
                bold: false,
                italic: false,
                mono: false,
            },
            Frag {
                text: "IP addresses".into(),
                bold: true,
                italic: false,
                mono: false,
            },
            Frag {
                text: ", connect devices".into(),
                bold: false,
                italic: false,
                mono: false,
            },
        ];
        let out = inline(&frags);
        eprintln!("inline: [{out}]");
        assert_eq!(
            out,
            "You will learn how to configure **IP addresses**, connect devices"
        );
    }
}
