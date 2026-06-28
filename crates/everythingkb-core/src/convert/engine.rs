use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use mdkit::Engine;

static ENGINE: OnceLock<Engine> = OnceLock::new();

fn engine() -> &'static Engine {
    ENGINE.get_or_init(Engine::with_defaults)
}

/// Convert a file to markdown via mdkit (pdfium, calamine, html, csv).
pub fn to_markdown(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "docx" || ext == "doc" {
        if let Ok(md) = undocx::convert(path) {
            return Ok(md);
        }
    }

    let doc = engine()
        .extract(path)
        .with_context(|| format!("mdkit extract {}", path.display()))?;
    Ok(doc.markdown)
}
