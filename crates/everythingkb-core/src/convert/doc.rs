use std::path::Path;

use anyhow::Result;

use crate::convert::{engine, pdf, resolve_doc_name};
use crate::kb::{KbPaths, WikiScope};
use crate::registry::Registry;

#[derive(Debug)]
pub struct ConvertResult {
    pub source_path: Option<std::path::PathBuf>,
    pub is_long_doc: bool,
    pub skipped: bool,
    pub file_hash: String,
    pub doc_name: String,
}

pub fn convert_document(
    src: &Path,
    kb: &KbPaths,
    registry: &Registry,
    force: bool,
    path_private: bool,
) -> Result<ConvertResult> {
    let file_hash = Registry::hash_file(src)?;
    let path_key = crate::registry::portable_path(src, &kb.root);
    let doc_name = resolve_doc_name(src, kb, registry)?;

    if !force {
        if let Some(rec) = registry.get(&path_key)? {
            if rec.file_hash == file_hash && rec.status == crate::registry::FileStatus::Indexed {
                return Ok(ConvertResult {
                    source_path: None,
                    is_long_doc: false,
                    skipped: true,
                    file_hash,
                    doc_name: rec.doc_name.unwrap_or(doc_name),
                });
            }
        }
    }

    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let config = kb.load_config()?;
    let scope = if path_private {
        WikiScope::Private
    } else {
        WikiScope::Public
    };
    let sources = kb.layout(scope).sources;

    let (source_path, is_long_doc) = match ext.as_str() {
        "md" | "markdown" | "txt" => {
            let markdown = std::fs::read_to_string(src)?;
            let dest = sources.join(format!("{doc_name}.md"));
            std::fs::write(&dest, markdown)?;
            (Some(dest), false)
        }
        "pdf" => {
            let page_count = pdf::pdf_page_count(src).unwrap_or(1);
            if page_count >= config.pageindex_threshold {
                return Ok(ConvertResult {
                    source_path: None,
                    is_long_doc: true,
                    skipped: false,
                    file_hash,
                    doc_name,
                });
            }
            let markdown = engine::to_markdown(src).unwrap_or_else(|e| {
                eprintln!("[convert] pdf {}: {e:#}", src.display());
                format!("# {doc_name}\n\n(PDF text extraction failed)\n")
            });
            let dest = sources.join(format!("{doc_name}.md"));
            std::fs::write(&dest, markdown)?;
            (Some(dest), false)
        }
        _ => {
            let markdown = engine::to_markdown(src).unwrap_or_else(|_| {
                std::fs::read_to_string(src).unwrap_or_else(|_| {
                    format!(
                        "# {}\n\n(Unsupported format `{}`)\n",
                        doc_name, ext
                    )
                })
            });
            let dest = sources.join(format!("{doc_name}.md"));
            std::fs::write(&dest, markdown)?;
            (Some(dest), false)
        }
    };

    Ok(ConvertResult {
        source_path,
        is_long_doc,
        skipped: false,
        file_hash,
        doc_name,
    })
}
