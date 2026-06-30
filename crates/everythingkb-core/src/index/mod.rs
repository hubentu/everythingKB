use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::convert::pdf_pages_text;
use crate::kb::{KbPaths, WikiScope};
use crate::llm::build_client;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub title: String,
    pub page_start: u32,
    pub page_end: u32,
    pub children: Vec<TreeNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageIndexDoc {
    pub doc_name: String,
    pub page_count: u32,
    pub tree: Vec<TreeNode>,
    pub pages: Vec<PageText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageText {
    pub page: u32,
    pub text: String,
}

pub fn build_page_index(
    kb: &KbPaths,
    doc_name: &str,
    raw_pdf: &Path,
    scope: WikiScope,
) -> Result<PageIndexDoc> {
    let layout = kb.layout(scope);
    let pageindex_dir = kb.pageindex_dir(scope);
    let pages = pdf_pages_text(raw_pdf)
        .with_context(|| format!("extract pages from {}", raw_pdf.display()))?;

    let page_count = pages.len() as u32;
    let tree = build_tree_with_llm(kb, doc_name, &pages)?;

    let doc = PageIndexDoc {
        doc_name: doc_name.into(),
        page_count,
        tree,
        pages: pages
            .into_iter()
            .map(|(page, text)| PageText { page, text })
            .collect(),
    };

    let json_path = pageindex_dir.join(format!("{doc_name}.json"));
    std::fs::create_dir_all(&pageindex_dir)?;
    std::fs::write(&json_path, serde_json::to_string_pretty(&doc)?)?;

    let source_json = layout.sources.join(format!("{doc_name}.json"));
    std::fs::write(&source_json, serde_json::to_string_pretty(&doc)?)?;

    let overview = tree_overview_markdown(&doc);
    let md_path = layout.sources.join(format!("{doc_name}.md"));
    std::fs::write(&md_path, overview)?;

    Ok(doc)
}

fn build_tree_with_llm(
    kb: &KbPaths,
    doc_name: &str,
    pages: &[(u32, String)],
) -> Result<Vec<TreeNode>> {
    let config = kb.load_config()?;
    let client = build_client(&config)?;

    let sample: String = pages
        .iter()
        .take(30)
        .map(|(p, t)| format!("## Page {p}\n{}\n", truncate(t, 800)))
        .collect();

    let system = "You build hierarchical table-of-contents trees for long PDFs. Return ONLY valid JSON.";
    let user = format!(
        "Document: {doc_name}\nPages: {}\n\nSample page text:\n{sample}\n\n\
         Return JSON: {{\"tree\": [{{\"title\": \"...\", \"page_start\": 1, \"page_end\": N, \
         \"children\": [...]}}]}}\n\
         Build 3-8 top-level sections covering all pages. page_start/page_end are inclusive.",
        pages.len()
    );

    let json = client.complete_json(&system, &user)?;
    if let Some(arr) = json["tree"].as_array() {
        let tree: Vec<TreeNode> = arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        if !tree.is_empty() {
            return Ok(tree);
        }
    }

    Ok(default_tree(pages.len() as u32))
}

fn default_tree(page_count: u32) -> Vec<TreeNode> {
    let chunk = (page_count / 4).max(1);
    let mut tree = Vec::new();
    let mut start = 1u32;
    while start <= page_count {
        let end = (start + chunk - 1).min(page_count);
        tree.push(TreeNode {
            title: format!("Pages {start}-{end}"),
            page_start: start,
            page_end: end,
            children: vec![],
            summary: None,
        });
        start = end + 1;
    }
    tree
}

pub fn tree_overview_markdown(doc: &PageIndexDoc) -> String {
    let mut out = format!("# {}\n\nPageIndex tree ({} pages)\n\n", doc.doc_name, doc.page_count);
    for node in &doc.tree {
        append_node(&mut out, node, 0);
    }
    out
}

fn append_node(out: &mut String, node: &TreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!(
        "{indent}- {} (pp. {}-{})\n",
        node.title, node.page_start, node.page_end
    ));
    for child in &node.children {
        append_node(out, child, depth + 1);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

pub fn load_page_index(kb: &KbPaths, doc_name: &str, scope: WikiScope) -> Result<Option<PageIndexDoc>> {
    let path = kb.pageindex_dir(scope).join(format!("{doc_name}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

pub fn node_text(doc: &PageIndexDoc, node: &TreeNode) -> String {
    doc.pages
        .iter()
        .filter(|p| p.page >= node.page_start && p.page <= node.page_end)
        .map(|p| format!("## Page {}\n{}", p.page, p.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn list_page_indexes(kb: &KbPaths, scope: WikiScope) -> Result<Vec<PathBuf>> {
    let dir = kb.pageindex_dir(scope);
    if !dir.exists() {
        return Ok(vec![]);
    }
    Ok(std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect())
}
