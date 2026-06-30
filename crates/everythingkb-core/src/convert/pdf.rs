use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use pdfium_render::prelude::*;

/// Matches pdfium-render 0.9.x; see pdfium-render CI.
const PDFIUM_TAG: &str = "7920";

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

fn cache_dir() -> PathBuf {
    std::env::var_os("EVERYTHINGKB_PDFIUM_CACHE")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                PathBuf::from(h)
                    .join(".cache")
                    .join("everythingkb")
                    .join(format!("pdfium-{PDFIUM_TAG}"))
            })
        })
        .unwrap_or_else(|| PathBuf::from("/tmp/everythingkb-pdfium"))
}

fn archive_name() -> Result<&'static str> {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match (os, arch) {
        ("linux", "x86_64") => Ok("pdfium-linux-x64.tgz"),
        ("linux", "aarch64") => Ok("pdfium-linux-arm64.tgz"),
        ("macos", "aarch64") => Ok("pdfium-mac-arm64.tgz"),
        ("macos", "x86_64") => Ok("pdfium-mac-x64.tgz"),
        ("windows", "x86_64") => Ok("pdfium-win-x64.tgz"),
        _ => anyhow::bail!("unsupported platform for bundled pdfium: {os}-{arch}"),
    }
}

fn lib_filename() -> &'static str {
    if cfg!(windows) {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

/// Download and cache libpdfium on first use.
fn ensure_lib() -> Result<PathBuf> {
    let dir = cache_dir();
    let lib = dir.join(lib_filename());
    if lib.exists() {
        return Ok(lib);
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    eprintln!("[pdfium] downloading chromium/{PDFIUM_TAG} (first run only)...");

    let url = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/{PDFIUM_TAG}/{}",
        archive_name()?
    );
    let tgz = dir.join("pdfium.tgz");
    let bytes = reqwest::blocking::get(&url)
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("download failed: {url}"))?
        .bytes()?;
    std::fs::write(&tgz, &bytes)?;

    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&tgz)
        .arg("-C")
        .arg(&dir)
        .status()
        .context("run tar")?;
    let _ = std::fs::remove_file(&tgz);
    if !status.success() {
        anyhow::bail!("failed to extract pdfium archive");
    }
    let extracted = dir.join("lib").join(lib_filename());
    if extracted.exists() {
        std::fs::rename(&extracted, &lib)?;
        let _ = std::fs::remove_dir(dir.join("lib"));
    }
    if !lib.exists() {
        anyhow::bail!("libpdfium not found after extract");
    }
    Ok(lib)
}

fn pdfium() -> Result<&'static Pdfium> {
    match PDFIUM.get_or_init(|| {
        let lib_path = ensure_lib().map_err(|e| e.to_string())?;
        let lib_dir = lib_path
            .parent()
            .ok_or_else(|| "pdfium library path has no parent".to_string())?;
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(lib_dir))
            .map(Pdfium::new)
            .map_err(|e| format!("bind pdfium: {e}"))
    }) {
        Ok(p) => Ok(p),
        Err(msg) => anyhow::bail!("{msg}"),
    }
}

/// Page count via pdfium-render.
pub fn pdf_page_count(path: &Path) -> Result<u32> {
    let doc = pdfium()?
        .load_pdf_from_file(path, None)
        .with_context(|| format!("open pdf {}", path.display()))?;
    Ok(doc.pages().len() as u32)
}

/// Full-document text for short PDF conversion.
pub fn pdf_full_text(path: &Path) -> Result<String> {
    let doc = pdfium()?
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
    let doc = pdfium()?
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn pdfium_reused_across_calls() {
        let Some(pdf) = std::env::var("EVERYTHINGKB_TEST_PDF").ok() else {
            return;
        };
        let path = Path::new(&pdf);
        let n = pdf_page_count(path).expect("page count");
        let text = pdf_full_text(path).expect("full text");
        assert!(n > 0);
        assert!(!text.is_empty());
    }
}
