use std::path::{Path, PathBuf};

use anyhow::Result;
use gray_matter::{Matter, Pod};

use crate::kb::{KbPaths, WikiLayout, WikiScope};

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
    format!(
        "---\ntype: Note\ntitle: {}\ntimestamp: {}\n---\n\n{body}",
        yaml_quote(title),
        okf_timestamp()
    )
}

/// OKF concept page (required `type`, recommended title/description/timestamp).
pub fn wrap_page_frontmatter(concept_type: &str, title: &str, description: &str, body: &str) -> String {
    format!(
        "---\ntype: {}\ntitle: {}\ndescription: {}\ntimestamp: {}\n---\n\n{body}",
        yaml_quote(concept_type),
        yaml_quote(title),
        yaml_quote(description),
        okf_timestamp()
    )
}

/// OKF document summary (`type: Document Summary`, `resource` = original file path).
pub fn wrap_summary_frontmatter(
    title: &str,
    resource: &str,
    description: &str,
    body: &str,
    is_private: bool,
) -> String {
    let tags = if is_private {
        "tags: [private]\nprivate: true\n"
    } else {
        ""
    };
    format!(
        "---\ntype: Document Summary\ntitle: {}\nresource: {}\ndescription: {}\ntimestamp: {}\n{tags}---\n\n{body}",
        yaml_quote(title),
        yaml_quote(resource),
        yaml_quote(description),
        okf_timestamp()
    )
}

/// Capitalize entity type for OKF `type` field (e.g. `person` → `Person`).
pub fn entity_okf_type(entity_type: &str) -> String {
    let mut c = entity_type.chars();
    match c.next() {
        None => "Entity".into(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// OKF cross-link: `[label](path/to/concept.md)`.
pub fn okf_link(path: &str, label: &str) -> String {
    let path = path.trim_start_matches('/');
    let path = if path.ends_with(".md") {
        path.to_string()
    } else {
        format!("{path}.md")
    };
    format!("[{label}]({path})")
}

pub fn okf_page_path(scope: WikiScope, kind: &str, slug: &str) -> String {
    format!("{}{kind}/{slug}.md", wiki_link_prefix(scope))
}

fn okf_timestamp() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = unix_to_utc(t);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// ponytail: civil UTC from unix secs; good enough for OKF timestamps
fn unix_to_utc(mut days: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (days % 60) as u32;
    days /= 60;
    let mi = (days % 60) as u32;
    days /= 60;
    let h = (days % 24) as u32;
    days /= 24;
    let mut y = 1970u32;
    loop {
        let diy = if is_leap(y) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        y += 1;
    }
    let md = [31u64, 28 + is_leap(y) as u64, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 1u32;
    for &dim in &md {
        if days < dim {
            break;
        }
        days -= dim;
        mo += 1;
    }
    (y, mo, (days + 1) as u32, h, mi, s)
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
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

/// Read `resource:` (OKF), with fallback to legacy `source:` / `sources:`.
pub fn frontmatter_sources(text: &str) -> Vec<String> {
    let Ok(parsed) = matter().parse(text) else {
        return Vec::new();
    };
    let Some(data) = parsed.data else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(s) = pod_str(&data, "resource").or_else(|| pod_str(&data, "source")) {
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

pub fn summary_sources(kb: &KbPaths, doc_name: &str, scope: WikiScope) -> Vec<String> {
    let path = summary_path(&kb.layout(scope).summaries, doc_name);
    read_page(&path)
        .ok()
        .map(|t| frontmatter_sources(&t))
        .unwrap_or_default()
}

pub fn wiki_link_prefix(scope: WikiScope) -> &'static str {
    match scope {
        WikiScope::Public => "",
        WikiScope::Private => "private/",
    }
}

pub fn wiki_page_id(scope: WikiScope, kind: &str, slug: &str) -> String {
    format!("{}{kind}/{slug}", wiki_link_prefix(scope))
}

/// Wiki page block for query/chat context (OKF path + resource URI).
pub fn format_context_page(page_path: &str, text: &str) -> String {
    let mut block = format!("## [{page_path}]({page_path})\n");
    for src in frontmatter_sources(text) {
        block.push_str(&format!("Resource: {src}\n"));
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
        .map(|s| format!("- {}", okf_link(&format!("{prefix}/{s}"), s)))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn known_targets(kb: &KbPaths, scope: WikiScope) -> Result<String> {
    let layout = kb.layout(scope);
    let prefix = wiki_link_prefix(scope);
    let mut lines = Vec::new();
    for (dir, kind) in [
        (&layout.summaries, "summaries"),
        (&layout.concepts, "concepts"),
        (&layout.entities, "entities"),
    ] {
        for slug in list_slugs(dir)? {
            lines.push(format!(
                "- {}",
                okf_link(&format!("{prefix}{kind}/{slug}"), &slug)
            ));
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
    scope: WikiScope,
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

    let p = wiki_link_prefix(scope);
    let summary_link = okf_link(&format!("{p}summaries/{doc_name}"), doc_name);
    let entry = format!(
        "\n## {summary_link}\n{doc_brief}\n\n\
         Concepts: {}\nEntities: {}\n",
        if concept_slugs.is_empty() {
            "(none)".into()
        } else {
            concept_slugs
                .iter()
                .map(|s| okf_link(&format!("{p}concepts/{s}"), s))
                .collect::<Vec<_>>()
                .join(", ")
        },
        if entity_slugs.is_empty() {
            "(none)".into()
        } else {
            entity_slugs
                .iter()
                .map(|s| okf_link(&format!("{p}entities/{s}"), s))
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    if body.contains(&format!("{p}summaries/{doc_name}.md"))
        || body.contains(&format!("[[{p}summaries/{doc_name}]]"))
    {
        return Ok(());
    }
    body.push_str(&entry);
    write_page(&index_path, &body)
}

pub fn add_related_link(
    layout: &WikiLayout,
    scope: WikiScope,
    slug: &str,
    doc_name: &str,
) -> Result<()> {
    let path = layout.concepts.join(format!("{slug}.md"));
    let path = if path.exists() {
        path
    } else {
        layout.entities.join(format!("{slug}.md"))
    };
    if !path.exists() {
        return Ok(());
    }
    let mut content = read_page(&path)?;
    let link = okf_link(
        &format!("{}summaries/{doc_name}", wiki_link_prefix(scope)),
        doc_name,
    );
    if content.contains(&link) {
        return Ok(());
    }
    content.push_str(&format!("\n\nSee also: {link}\n"));
    write_page(&path, &content)
}

pub fn backlink_summary(
    layout: &WikiLayout,
    scope: WikiScope,
    doc_name: &str,
    concept_slugs: &[String],
) -> Result<()> {
    let path = summary_path(&layout.summaries, doc_name);
    if !path.exists() {
        return Ok(());
    }
    let mut content = read_page(&path)?;
    let p = wiki_link_prefix(scope);
    for slug in concept_slugs {
        let link = okf_link(&format!("{p}concepts/{slug}"), slug);
        if !content.contains(&link) && !content.contains(&format!("[[{p}concepts/{slug}]]")) {
            content.push_str(&format!("\n- Related concept: {link}"));
        }
    }
    write_page(&path, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn okf_summary_frontmatter() {
        let md = wrap_summary_frontmatter("doc", "/tmp/a.pdf", "brief", "body", false);
        assert!(md.contains("type: Document Summary"));
        assert!(md.contains("resource: /tmp/a.pdf"));
    }

    #[test]
    fn okf_link_format() {
        assert_eq!(
            okf_link("concepts/foo", "Foo"),
            "[Foo](concepts/foo.md)"
        );
    }
}
