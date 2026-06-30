use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use mdkit::{Document, Engine, Error as MdkitError, Extractor, Result as MdkitResult};

use super::pdf;

static ENGINE: OnceLock<Engine> = OnceLock::new();

/// PDF via pdfium-auto (auto-download/cache); other formats via mdkit backends.
struct PdfiumExtractor;

impl Extractor for PdfiumExtractor {
    fn extensions(&self) -> &[&'static str] {
        &["pdf"]
    }

    fn extract(&self, path: &Path) -> MdkitResult<Document> {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document");
        let text = pdf::pdf_full_text(path).map_err(|e| {
            MdkitError::ParseError(format!("pdfium-render: {e}"))
        })?;
        Ok(Document::new(format!("# {stem}\n\n{text}")))
    }

    fn name(&self) -> &'static str {
        "pdfium-render"
    }
}

fn engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let mut engine = Engine::new();
        engine.register(Box::new(PdfiumExtractor));
        engine.register(Box::new(mdkit::calamine::CalamineExtractor::new()));
        engine.register(Box::new(mdkit::csv::CsvExtractor::new()));
        engine.register(Box::new(mdkit::html::Html2mdExtractor::new()));
        engine
    })
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
