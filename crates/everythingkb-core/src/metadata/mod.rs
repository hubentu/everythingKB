mod binary;
mod image;

use std::path::Path;

use anyhow::Result;

use crate::kb::{KbPaths, WikiScope};

pub use binary::profile_stub;
pub use image::write_image_summary;

pub fn write_metadata_stub(
    src: &Path,
    kb: &KbPaths,
    doc_name: &str,
    path_private: bool,
) -> Result<std::path::PathBuf> {
    let scope = if path_private {
        WikiScope::Private
    } else {
        WikiScope::Public
    };
    let markdown = profile_stub(src)?;
    let dest = kb.layout(scope).sources.join(format!("{doc_name}.md"));
    std::fs::write(&dest, markdown)?;
    Ok(dest)
}
