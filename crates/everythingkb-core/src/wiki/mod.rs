use std::path::{Path, PathBuf};

use anyhow::Result;
use gray_matter::{Matter, Pod};

static MATTER: std::sync::OnceLock<Matter<gray_matter::engine::YAML>> = std::sync::OnceLock::new();

fn matter() -> &'static Matter<gray_matter::engine::YAML> {
    MATTER.get_or_init(Matter::new)
}

pub fn read_page(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))
}

pub fn write_page(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

pub fn append_log(wiki: &Path, action: &str, name: &str) -> Result<()> {
    let log = wiki.join("log.md");
    let line = format!("\n- [{action}] {name}\n");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

pub fn list_slugs(dir: &Path) -> Result<Vec<String>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    Ok(walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        .filter_map(|e| {
            e.path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect())
}

pub fn list_concept_slugs(concepts_dir: &Path) -> Result<Vec<String>> {
    list_slugs(concepts_dir)
}

pub fn list_entity_slugs(entities_dir: &Path) -> Result<Vec<String>> {
    list_slugs(entities_dir)
}

pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub fn wrap_frontmatter(title: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n---\n\n{body}")
}

/// Concept/entity page with OpenKB-style frontmatter.
pub fn wrap_page_frontmatter(title: &str, description: &str, body: &str) -> String {
    format!(
        "---\ntitle: {title}\ndescription: {}\n---\n\n{body}",
        yaml_quote(description)
    )
}

pub fn wrap_summary_frontmatter(
    title: &str,
    source_path: &str,
    description: &str,
    body: &str,
) -> String {
    format!(
        "---\ntitle: {title}\nsource: {}\ndescription: {}\n---\n\n{body}",
        yaml_quote(source_path),
        yaml_quote(description)
    )
}

fn yaml_quote(s: &str) -> String {
    if s.contains('\n') || s.contains('"') || s.contains(':') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

pub fn source_path_display(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Read `source:` (or `sources:` array) from page frontmatter.
pub fn frontmatter_sources(text: &str) -> Vec<String> {
    let Ok(parsed) = matter().parse(text) else {
        return Vec::new();
    };
    let Some(data) = parsed.data else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(s) = pod_str(&data, "source") {
        if !s.is_empty() {
            out.push(s);
        }
    }
    if let Pod::Hash(h) = &data {
        if let Some(Pod::Array(arr)) = h.get("sources") {
            for item in arr {
                if let Ok(s) = item.as_string() {
                    if !s.is_empty() {
                        out.push(s);
                    }
                }
            }
        }
    }
    out
}

pub fn summary_sources(kb: &crate::kb::KbPaths, doc_name: &str) -> Vec<String> {
    let path = summary_path(&kb.summaries, doc_name);
    read_page(&path)
        .ok()
        .map(|t| frontmatter_sources(&t))
        .unwrap_or_default()
}

/// Wiki page block for query/chat context (wikilink id + original file paths).
pub fn format_context_page(wiki_id: &str, text: &str) -> String {
    let mut block = format!("## [[{wiki_id}]]\n");
    for src in frontmatter_sources(text) {
        block.push_str(&format!("Original file: {src}\n"));
    }
    block.push('\n');
    block.push_str(text);
    block.push_str("\n\n");
    block
}

fn pod_str(data: &Pod, key: &str) -> Option<String> {
    match data {
        Pod::Hash(h) => h.get(key).and_then(|v| v.as_string().ok()),
        _ => None,
    }
}

pub fn summary_path(summaries: &Path, doc_name: &str) -> PathBuf {
    summaries.join(format!("{doc_name}.md"))
}

pub fn concept_path(concepts: &Path, slug: &str) -> PathBuf {
    concepts.join(format!("{slug}.md"))
}

pub fn entity_path(entities: &Path, slug: &str) -> PathBuf {
    entities.join(format!("{slug}.md"))
}

pub fn agents_md(wiki: &Path) -> String {
    let path = wiki.join("AGENTS.md");
    read_page(&path).unwrap_or_else(|_| {
        "# Wiki Agent Instructions\n\nSummaries, concepts, entities, sources.\n".into()
    })
}

pub fn briefs_for_dir(dir: &Path, prefix: &str) -> Result<String> {
    let slugs = list_slugs(dir)?;
    if slugs.is_empty() {
        return Ok("(none)".into());
    }
    Ok(slugs
        .iter()
        .map(|s| format!("- [[{prefix}/{s}]]"))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn known_targets(kb: &crate::kb::KbPaths) -> Result<String> {
    let mut lines = Vec::new();
    for (dir, prefix) in [
        (&kb.summaries, "summaries"),
        (&kb.concepts, "concepts"),
        (&kb.entities, "entities"),
    ] {
        for slug in list_slugs(dir)? {
            lines.push(format!("- [[{prefix}/{slug}]]"));
        }
    }
    if lines.is_empty() {
        Ok("(none)".into())
    } else {
        Ok(lines.join("\n"))
    }
}

pub fn update_index(
    wiki: &Path,
    doc_name: &str,
    concept_slugs: &[String],
    entity_slugs: &[String],
    doc_brief: &str,
) -> Result<()> {
    let index_path = wiki.join("index.md");
    let mut body = if index_path.exists() {
        read_page(&index_path)?
    } else {
        "# Knowledge Base Index\n\n".into()
    };

    let entry = format!(
        "\n## [[summaries/{doc_name}]]\n{doc_brief}\n\n\
         Concepts: {}\nEntities: {}\n",
        if concept_slugs.is_empty() {
            "(none)".into()
        } else {
            concept_slugs
                .iter()
                .map(|s| format!("[[concepts/{s}]]"))
                .collect::<Vec<_>>()
                .join(", ")
        },
        if entity_slugs.is_empty() {
            "(none)".into()
        } else {
            entity_slugs
                .iter()
                .map(|s| format!("[[entities/{s}]]"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    if body.contains(&format!("[[summaries/{doc_name}]]")) {
        return Ok(());
    }
    body.push_str(&entry);
    write_page(&index_path, &body)
}

pub fn add_related_link(
    concepts_or_entities: &Path,
    slug: &str,
    doc_name: &str,
) -> Result<()> {
    let path = concepts_or_entities.join(format!("{slug}.md"));
    if !path.exists() {
        return Ok(());
    }
    let mut content = read_page(&path)?;
    let link = format!("[[summaries/{doc_name}]]");
    if content.contains(&link) {
        return Ok(());
    }
    content.push_str(&format!("\n\nSee also: {link}\n"));
    write_page(&path, &content)
}

pub fn backlink_summary(summaries: &Path, doc_name: &str, concept_slugs: &[String]) -> Result<()> {
    let path = summary_path(summaries, doc_name);
    if !path.exists() {
        return Ok(());
    }
    let mut content = read_page(&path)?;
    for slug in concept_slugs {
        let link = format!("[[concepts/{slug}]]");
        if !content.contains(&link) {
            content.push_str(&format!("\n- Related concept: {link}"));
        }
    }
    write_page(&path, &content)
}
