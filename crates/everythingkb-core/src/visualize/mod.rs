//! Wikilink graph → self-contained interactive HTML (OpenKB visualize parity).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use gray_matter::{Matter, Pod};
use regex::Regex;
use serde::Serialize;

use crate::kb::KbPaths;

const CONTENT_DIRS: &[(&str, &str)] = &[
    ("summaries", "Summary"),
    ("concepts", "Concept"),
    ("entities", "Entity"),
];

const GRAPH_TEMPLATE: &str = include_str!("../../assets/graph.html");

#[derive(Debug, Clone, Serialize)]
struct GraphNode {
    id: String,
    label: String,
    #[serde(rename = "type")]
    node_type: String,
    description: String,
    sources: Vec<String>,
    out: u32,
    #[serde(rename = "in")]
    in_degree: u32,
}

#[derive(Debug, Clone, Serialize)]
struct GraphEdge {
    source: String,
    target: String,
}

#[derive(Debug, Clone, Serialize)]
struct Graph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    types: Vec<String>,
}

fn build_graph(kb: &KbPaths) -> Result<Graph> {
    let matter = Matter::<gray_matter::engine::YAML>::new();
    let wikilink_re = Regex::new(r"\[\[([^\]]+)\]\]")?;

    let mut nodes: HashMap<String, GraphNode> = HashMap::new();
    let mut texts: HashMap<String, String> = HashMap::new();

    for (subdir, default_type) in CONTENT_DIRS {
        let dir = kb.wiki.join(subdir);
        if !dir.exists() {
            continue;
        }
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
            .collect();
        paths.sort();

        for path in paths {
            let stem = path.file_stem().unwrap().to_string_lossy();
            let nid = format!("{subdir}/{stem}");
            let text = std::fs::read_to_string(&path)?;
            texts.insert(nid.clone(), text.clone());

            let (node_type, description, sources) = parse_frontmatter(&matter, &text, default_type);
            nodes.insert(
                nid.clone(),
                GraphNode {
                    id: nid,
                    label: stem.into_owned(),
                    node_type,
                    description,
                    sources,
                    out: 0,
                    in_degree: 0,
                },
            );
        }
    }

    let norm: HashMap<String, String> = nodes
        .keys()
        .map(|nid| (normalize_target(nid), nid.clone()))
        .collect();

    let mut edges = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for (src, text) in &texts {
        for cap in wikilink_re.captures_iter(text) {
            let raw = cap.get(1).unwrap().as_str();
            let target = raw.split('|').next().unwrap_or(raw).trim();
            let Some(tgt) = norm.get(&normalize_target(target)) else {
                continue;
            };
            if tgt == src {
                continue;
            }
            let key = (src.clone(), tgt.clone());
            if !seen.insert(key) {
                continue;
            }
            edges.push(GraphEdge {
                source: src.clone(),
                target: tgt.clone(),
            });
            if let Some(n) = nodes.get_mut(src) {
                n.out += 1;
            }
            if let Some(n) = nodes.get_mut(tgt) {
                n.in_degree += 1;
            }
        }
    }

    let mut node_list: Vec<GraphNode> = nodes.into_values().collect();
    node_list.sort_by(|a, b| a.id.cmp(&b.id));

    let mut types: Vec<String> = node_list.iter().map(|n| n.node_type.clone()).collect();
    types.sort();
    types.dedup();

    Ok(Graph {
        nodes: node_list,
        edges,
        types,
    })
}

fn parse_frontmatter(
    matter: &Matter<gray_matter::engine::YAML>,
    text: &str,
    default_type: &str,
) -> (String, String, Vec<String>) {
    let mut node_type = default_type.to_string();
    let mut description = String::new();
    let mut sources = Vec::new();
    let mut body = text.to_string();

    if let Ok(parsed) = matter.parse(text) {
        body = parsed.content;
        if let Some(data) = parsed.data {
            if let Some(t) = pod_str(&data, "type") {
                if !t.is_empty() {
                    node_type = t;
                }
            }
            if let Some(d) = pod_str(&data, "description") {
                description = d;
            }
            if let Some(s) = pod_str(&data, "source") {
                sources.push(s);
            }
            if let Pod::Hash(h) = &data {
                if let Some(Pod::Array(arr)) = h.get("sources") {
                    for item in arr {
                        if let Ok(s) = item.as_string() {
                            sources.push(s);
                        }
                    }
                }
            }
            if let Some(ft) = pod_str(&data, "full_text") {
                sources.insert(0, ft);
            }
            if description.is_empty() {
                if let Some(title) = pod_str(&data, "title") {
                    if !looks_like_slug(&title) {
                        description = title;
                    }
                }
            }
        }
    }

    if description.is_empty() {
        description = excerpt_from_body(&body);
    }

    (node_type, description, sources)
}

fn looks_like_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(' ')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn excerpt_from_body(content: &str) -> String {
    for block in content.split("\n\n") {
        let line = match block.lines().find(|l| !l.trim().is_empty()) {
            Some(l) => l.trim(),
            None => continue,
        };
        if line.starts_with('#') {
            continue;
        }
        let mut out: String = line.chars().take(240).collect();
        if line.chars().count() > 240 {
            out.push('…');
        }
        return out;
    }
    String::new()
}

fn pod_str(data: &Pod, key: &str) -> Option<String> {
    let Pod::Hash(h) = data else {
        return None;
    };
    h.get(key)
        .and_then(|p| p.as_string().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_target(target: &str) -> String {
    target
        .to_lowercase()
        .replace('_', "-")
        .split('/')
        .map(|seg| {
            seg.split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn render_html(graph: &Graph) -> Result<String> {
    let mut data = serde_json::to_string(graph)?;
    data = data.replace("</", "<\\/");
    Ok(GRAPH_TEMPLATE.replace("__GRAPH_DATA__", &data))
}

pub fn write_visualization(kb: &KbPaths, output: &Path) -> Result<(usize, usize)> {
    let graph = build_graph(kb)?;
    let n_nodes = graph.nodes.len();
    let n_edges = graph.edges.len();
    let html = render_html(&graph)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, html)?;
    Ok((n_nodes, n_edges))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_wikilink_target() {
        assert_eq!(normalize_target("concepts/Gist_Memory"), "concepts/gist-memory");
    }

    #[test]
    fn excerpt_from_first_paragraph() {
        let body = "# Title\n\nFirst sentence here.\n\nMore text.";
        assert_eq!(
            super::excerpt_from_body(body),
            "First sentence here."
        );
    }

    #[test]
    fn uses_title_as_description_when_sentence() {
        let text = "---\ntitle: ADCC is a mechanism\n---\n\n# ADCC\n\nBody.";
        let matter = Matter::<gray_matter::engine::YAML>::new();
        let (_, desc, _) = super::parse_frontmatter(&matter, text, "Concept");
        assert!(desc.contains("mechanism"));
    }

    #[test]
    fn render_injects_graph_json() {
        let graph = Graph {
            nodes: vec![GraphNode {
                id: "summaries/a".into(),
                label: "a".into(),
                node_type: "Summary".into(),
                description: String::new(),
                sources: vec![],
                out: 0,
                in_degree: 0,
            }],
            edges: vec![],
            types: vec!["Summary".into()],
        };
        let html = render_html(&graph).unwrap();
        assert!(html.contains("\"summaries/a\""));
        assert!(!html.contains("__GRAPH_DATA__"));
    }
}
