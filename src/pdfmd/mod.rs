//! Subject PDF → markdown conversion.
//!
//! 42 subjects are LaTeX PDFs with a very regular layout (Latin Modern fonts,
//! fixed sizes and accent colors). [`extract`] walks the content streams into
//! positioned spans, images and stroke segments; [`parse`] rebuilds the
//! document structure (chapters, bullets, note boxes, tables…); [`render`]
//! emits the markdown. Extracted figures are re-encoded as PNGs.

pub(crate) mod extract;
mod images;
pub(crate) mod parse;
mod render;

/// A converted subject: the markdown plus every extracted figure.
#[derive(Debug, Default)]
pub struct Subject {
    pub markdown: String,
    pub images: Vec<SubjectImage>,
}

#[derive(Debug)]
pub struct SubjectImage {
    /// File name as referenced from the markdown (`Mandatory_part2.png`).
    pub name: String,
    /// PNG-encoded figure bytes.
    pub bytes: Vec<u8>,
}

/// Convert a 42 subject PDF into markdown + figures.
pub fn convert(pdf_bytes: &[u8]) -> Result<Subject, String> {
    use pdf::object::Resolve as _;
    let backend = pdf_bytes.to_vec();
    let file = pdf::file::FileOptions::uncached()
        .load(backend)
        .map_err(|error| format!("parse pdf: {error}"))?;

    let mut pages = Vec::new();
    for (index, page) in file.pages().enumerate() {
        let page = page.map_err(|error| format!("page {index}: {error}"))?;
        let resolve = file.resolver();
        let extracted = extract::page(&page, &resolve);
        pages.push(extracted);
    }

    let doc = parse::document(&pages);
    let (markdown, image_slots) = render::markdown(&doc);

    // Materialize every referenced figure as a PNG.
    let mut images = Vec::new();
    let resolve = file.resolver();
    for slot in image_slots {
        if let Ok(xobject) = resolve.get(slot.xref)
            && let Some(bytes) = images::to_png(&xobject, &resolve)
        {
            images.push(SubjectImage {
                name: slot.name,
                bytes,
            });
        }
    }

    Ok(Subject { markdown, images })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Option<Vec<u8>> {
        std::fs::read(format!(
            "{}/docs/42pdf2md/tests/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    }

    #[test]
    fn netpractice_style() {
        let Some(bytes) = fixture("NetPractice.pdf") else {
            eprintln!("fixture missing; skipping");
            return;
        };
        let subject = convert(&bytes).expect("convert");
        std::fs::write("/tmp/netpractice-ours.md", &subject.markdown).unwrap();
        for image in &subject.images {
            std::fs::write(format!("/tmp/{}", image.name), &image.bytes).unwrap();
        }
        eprintln!("--- markdown ({} images) ---", subject.images.len());
        eprintln!("{}", subject.markdown);
    }
}
