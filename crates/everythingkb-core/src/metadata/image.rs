use std::path::Path;

use anyhow::{Context, Result};

use crate::compile::prompts;
use crate::kb::KbPaths;
use crate::llm::build_client;
use crate::metadata::binary::profile_stub;

pub fn write_image_summary(src: &Path, kb: &KbPaths, doc_name: &str) -> Result<std::path::PathBuf> {
    let config = kb.load_config()?;
    let client = build_client(&config)?;
    let language = &config.language;
    let schema = crate::wiki::agents_md(&kb.wiki);
    let system = prompts::system_schema(language, &schema);
    let user = prompts::image_summary_user(doc_name, src);

    let description = client
        .complete_image(&system, &user, src)
        .with_context(|| format!("vision summary {}", src.display()))?;

    let mut markdown = profile_stub(src)?;
    markdown.push_str("\n\n## Visual description\n\n");
    markdown.push_str(description.trim());
    markdown.push('\n');

    let dest = kb.sources.join(format!("{doc_name}.md"));
    std::fs::write(&dest, markdown)?;
    Ok(dest)
}
