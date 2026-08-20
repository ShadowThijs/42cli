//! Subject PDFs as in-TUI markdown: fetch the attachment, convert it via
//! [`crate::pdfmd`], and cache text + figures under the cache dir so the
//! second open is instant (and the files are reusable outside the TUI).

use std::path::PathBuf;
use std::time::Duration;

use super::{Api, ApiError, ApiResult};

/// `~/.cache/42cli/subjects/{slug}.md` plus the sibling figures directory.
pub(crate) fn subject_paths(slug: &str) -> (PathBuf, PathBuf) {
    let safe = slug.replace(['/', '\\'], "_");
    let dir = crate::config::cache_dir().join("subjects");
    (dir.join(format!("{safe}.md")), dir.join(format!("{safe}.files")))
}

impl Api {
    /// Convert a subject attachment to markdown, serving a cached copy when
    /// one exists. Figures are written next to the markdown; the markdown
    /// references them by file name.
    pub async fn subject_markdown(&self, slug: &str, url: &str) -> ApiResult<String> {
        let (markdown_path, figures_dir) = subject_paths(slug);

        if let Ok(cached) = std::fs::read_to_string(&markdown_path)
            && !cached.is_empty()
        {
            return Ok(cached);
        }

        let resp = self
            .http
            .get(url)
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(ApiError::from_response("cdn", resp).await);
        }
        let bytes = resp.bytes().await?;

        // The PDF walk + file writes are blocking; keep them off the async
        // workers.
        tokio::task::spawn_blocking(move || {
            let convert = crate::pdfmd::convert(&bytes).map_err(ApiError::Other)?;
            std::fs::create_dir_all(&figures_dir).map_err(|error| {
                ApiError::Other(format!("create {}: {error}", figures_dir.display()))
            })?;
            for image in &convert.images {
                let _ = std::fs::write(figures_dir.join(&image.name), &image.bytes);
            }
            std::fs::write(&markdown_path, &convert.markdown).map_err(|error| {
                ApiError::Other(format!("write {}: {error}", markdown_path.display()))
            })?;
            Ok(convert.markdown)
        })
        .await
        .map_err(|error| ApiError::Other(format!("converter panicked: {error}")))?
    }
}
