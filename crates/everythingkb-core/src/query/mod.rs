use anyhow::Result;

use crate::compile::prompts;
use crate::index::{self, PageIndexDoc, TreeNode};
use crate::kb::KbPaths;
use crate::llm::build_client;
use crate::sessions::SessionStore;
use crate::wiki;

pub fn query(kb: &KbPaths, question: &str) -> Result<String> {
    let config = kb.load_config()?;
    let client = build_client(&config)?;
    let mut context = gather_wiki_context(kb)?;
    context.push_str(&gather_tree_context(kb, question, &*client, &config.language)?);

    let schema = wiki::agents_md(&kb.wiki);
    let system = prompts::system_schema(&config.language, &schema);
    let user = prompts::query_user(question, &context);
    client.complete(&system, &user)
}

fn gather_tree_context(
    kb: &KbPaths,
    question: &str,
    client: &dyn crate::llm::LlmClient,
    language: &str,
) -> Result<String> {
    let indexes = index::list_page_indexes(kb)?;
    if indexes.is_empty() {
        return Ok(String::new());
    }

    let mut tree_summaries = Vec::new();
    let mut loaded_docs: Vec<PageIndexDoc> = Vec::new();

    for path in &indexes {
        let raw = std::fs::read_to_string(path)?;
        if let Ok(doc) = serde_json::from_str::<PageIndexDoc>(&raw) {
            let outline: String = doc
                .tree
                .iter()
                .map(|n| format!("  - {} (pp. {}-{})", n.title, n.page_start, n.page_end))
                .collect::<Vec<_>>()
                .join("\n");
            tree_summaries.push(format!("Document: {}\n{outline}", doc.doc_name));
            loaded_docs.push(doc);
        }
    }

    let trees = tree_summaries.join("\n\n");
    let system = prompts::system_schema(language, "");
    let sel = client.complete_json(
        &system,
        &prompts::tree_select_user(question, &trees),
    )?;

    let mut parts = vec!["\n## Long document excerpts\n".to_string()];
    if let Some(selections) = sel["selections"].as_array() {
        for item in selections {
            let doc_name = item["doc_name"].as_str().unwrap_or("");
            let title = item["title"].as_str().unwrap_or("");
            if let Some(doc) = loaded_docs.iter().find(|d| d.doc_name == doc_name) {
                if let Some(node) = find_node(&doc.tree, title) {
                    let mut header = format!("### [[summaries/{doc_name}]]: {title}\n");
                    for src in wiki::summary_sources(kb, doc_name) {
                        header.push_str(&format!("Original file: {src}\n"));
                    }
                    parts.push(format!(
                        "{header}{}",
                        index::node_text(doc, node)
                    ));
                }
            }
        }
    }

    Ok(parts.join("\n"))
}

fn find_node<'a>(nodes: &'a [TreeNode], title: &str) -> Option<&'a TreeNode> {
    for n in nodes {
        if n.title.eq_ignore_ascii_case(title) {
            return Some(n);
        }
        if let Some(found) = find_node(&n.children, title) {
            return Some(found);
        }
    }
    None
}

fn gather_wiki_context(kb: &KbPaths) -> Result<String> {
    let mut parts = Vec::new();
    for (dir, prefix) in [
        (&kb.summaries, "summaries"),
        (&kb.concepts, "concepts"),
        (&kb.entities, "entities"),
    ] {
        if !dir.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .take(30)
        {
            let slug = entry
                .path()
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let wiki_id = format!("{prefix}/{slug}");
            let text = wiki::read_page(entry.path())?;
            parts.push(wiki::format_context_page(&wiki_id, &text));
        }
    }
    Ok(parts.join("\n"))
}

pub fn chat_turn(kb: &KbPaths, session_id: &str, question: &str) -> Result<String> {
    let sessions_path = kb.meta.join("sessions.db");
    let store = SessionStore::open(&sessions_path)?;
    let history = store.get_history(session_id)?;
    let answer = query_with_history(kb, question, &history)?;
    store.append_turn(session_id, question, &answer)?;
    Ok(answer)
}

fn query_with_history(kb: &KbPaths, question: &str, history: &str) -> Result<String> {
    let config = kb.load_config()?;
    let client = build_client(&config)?;
    let mut context = gather_wiki_context(kb)?;
    context.push_str(&gather_tree_context(kb, question, &*client, &config.language)?);
    let schema = wiki::agents_md(&kb.wiki);
    let system = prompts::system_schema(&config.language, &schema);
    let user = prompts::chat_user(history, &context, question);
    client.complete(&system, &user)
}

pub fn chat_repl(kb: &KbPaths, session_id: &str) -> Result<()> {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    writeln!(stdout, "everythingKB chat (session: {session_id}). Type 'exit' to quit.")?;
    stdout.flush()?;

    loop {
        write!(stdout, "\n> ")?;
        stdout.flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let q = line.trim();
        if q.is_empty() {
            continue;
        }
        if q.eq_ignore_ascii_case("exit") || q.eq_ignore_ascii_case("quit") {
            break;
        }
        match chat_turn(kb, session_id, q) {
            Ok(a) => writeln!(stdout, "\n{a}")?,
            Err(e) => writeln!(stdout, "\n[error] {e:#}")?,
        }
        stdout.flush()?;
    }
    Ok(())
}
