use std::path::Path;

use anyhow::{Context, Result};
use pdfium_render::prelude::*;

fn bind_pdfium() -> Result<Pdfium> {
    Ok(Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .context("pdfium library not found; install libpdfium or set LD_LIBRARY_PATH")?,
    ))
}

/// Page count via pdfium-render.
pub fn pdf_page_count(path: &Path) -> Result<u32> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .with_context(|| format!("open pdf {}", path.display()))?;
    Ok(doc.pages().len() as u32)
}

/// Full-document text for short PDF conversion.
pub fn pdf_full_text(path: &Path) -> Result<String> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .with_context(|| format!("open pdf {}", path.display()))?;
    let mut out = String::new();
    for page in doc.pages().iter() {
        if let Ok(text) = page.text().map(|t| t.all()) {
            if !text.is_empty() {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&text);
            }
        }
    }
    Ok(out)
}

/// Per-page text for long-document tree indexing.
pub fn pdf_pages_text(path: &Path) -> Result<Vec<(u32, String)>> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(path, None)
        .with_context(|| format!("open pdf {}", path.display()))?;
    let mut out = Vec::new();
    for (idx, page) in doc.pages().iter().enumerate() {
        let text = page
            .text()
            .map(|t| t.all())
            .unwrap_or_default();
        out.push((idx as u32 + 1, text));
    }
    Ok(out)
}
