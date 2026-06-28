mod doc;
mod engine;
mod pdf;

use std::path::Path;

use anyhow::Result;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::kb::KbPaths;
use crate::registry::{portable_path, Registry};

pub use doc::ConvertResult;
pub use pdf::{pdf_full_text, pdf_page_count, pdf_pages_text};

pub fn resolve_doc_name(src: &Path, kb: &KbPaths, registry: &Registry) -> Result<String> {
    let path_key = portable_path(src, &kb.root);
    if let Some(rec) = registry.get(&path_key)? {
        if let Some(name) = rec.doc_name {
            return Ok(name);
        }
    }
    Ok(sanitize_stem(
        src.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("document"),
        &path_key,
        registry,
    ))
}

fn sanitize_stem(stem: &str, path_key: &str, registry: &Registry) -> String {
    let re = Regex::new(r"[^\w\-]+").expect("valid regex");
    let candidate = re
        .replace_all(stem, "-")
        .trim_matches('-')
        .to_string();
    let candidate = if candidate.is_empty() {
        "document".into()
    } else {
        candidate
    };

    if name_taken(&candidate, registry) {
        let digest = format!("{:x}", Sha256::digest(path_key.as_bytes()));
        format!("{}-{}", candidate, &digest[..8])
    } else {
        candidate
    }
}

fn name_taken(candidate: &str, registry: &Registry) -> bool {
    registry
        .list_indexed()
        .ok()
        .map(|rows| {
            rows.iter()
                .any(|r| r.doc_name.as_deref() == Some(candidate))
        })
        .unwrap_or(false)
}

pub fn convert_document(src: &Path, kb: &KbPaths, registry: &Registry, force: bool) -> Result<ConvertResult> {
    doc::convert_document(src, kb, registry, force)
}
