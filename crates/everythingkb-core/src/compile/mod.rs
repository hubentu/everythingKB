pub mod prompts;

use std::path::Path;

use anyhow::Result;

use crate::index;
use crate::kb::{KbPaths, WikiLayout, WikiScope};
use crate::llm::{build_client, LlmClient};
use crate::privacy;
use crate::wiki;

pub struct CompileOutcome {
    pub doc_name: String,
    pub summary_path: std::path::PathBuf,
    pub private: bool,
}

pub fn compile_short_doc(
    kb: &KbPaths,
    doc_name: &str,
    source_path: &Path,
    original_path: &Path,
    path_private: bool,
) -> Result<CompileOutcome> {
    let content = wiki::read_page(source_path)?;
    compile_from_content(kb, doc_name, &content, original_path, false, path_private)
}

fn compile_from_content(
    kb: &KbPaths,
    doc_name: &str,
    content: &str,
    original_path: &Path,
    is_long: bool,
    path_private: bool,
) -> Result<CompileOutcome> {
    let config = kb.load_config()?;
    let client = build_client(&config)?;
    let language = &config.language;
    let schema = wiki::agents_md(&kb.wiki);
    let system = prompts::system_schema(language, &schema);

    let summary_user = prompts::summary_user(doc_name, content);
    eprintln!("[compile] {doc_name}: summary...");
    let summary_json = client.complete_json(&system, &summary_user)?;
    let doc_private = privacy::doc_is_private(path_private, &summary_json, config.private_detect);
    let scope = if doc_private {
        WikiScope::Private
    } else {
        WikiScope::Public
    };
    if !path_private && doc_private {
        promote_sources(kb, doc_name)?;
    }
    let layout = kb.layout(scope);

    let description = summary_json["description"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let summary_body = summary_json["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let mut concept_slugs = Vec::new();
    let mut entity_slugs = Vec::new();

    eprintln!("[compile] {doc_name}: concepts/entities...");
    apply_plan(
        &layout,
        scope,
        doc_name,
        &summary_body,
        &*client,
        &system,
        &config.entity_types,
        &mut concept_slugs,
        &mut entity_slugs,
    )?;

    eprintln!("[compile] {doc_name}: final summary...");
    let final_summary = client
        .complete(
            &system,
            &format!(
                "Document summary draft:\n{summary_body}\n\n{}",
                prompts::summary_rewrite_user()
            ),
        )
        .unwrap_or(summary_body.clone());

    let summary_path = wiki::summary_path(&layout.summaries, doc_name);
    let source = wiki::source_path_display(original_path);
    let summary_md = wiki::wrap_summary_frontmatter(
        doc_name,
        &source,
        &description,
        &final_summary,
        doc_private,
    );
    wiki::write_page(&summary_path, &summary_md)?;

    wiki::backlink_summary(&layout, scope, doc_name, &concept_slugs)?;
    wiki::update_index(
        &layout.wiki,
        scope,
        doc_name,
        &concept_slugs,
        &entity_slugs,
        &description,
    )?;

    let action = if is_long { "compile-long" } else { "compile" };
    wiki::append_log(&layout.wiki, action, doc_name)?;
    Ok(CompileOutcome {
        doc_name: doc_name.into(),
        summary_path,
        private: doc_private,
    })
}

fn promote_sources(kb: &KbPaths, doc_name: &str) -> Result<()> {
    let public = kb.layout(WikiScope::Public);
    let private = kb.layout(WikiScope::Private);
    for ext in ["md", "json"] {
        let from = public.sources.join(format!("{doc_name}.{ext}"));
        let to = private.sources.join(format!("{doc_name}.{ext}"));
        if from.exists() {
            std::fs::create_dir_all(to.parent().unwrap())?;
            std::fs::rename(from, to)?;
        }
    }
    let pub_idx = kb.pageindex_dir(WikiScope::Public).join(format!("{doc_name}.json"));
    let priv_idx = kb
        .pageindex_dir(WikiScope::Private)
        .join(format!("{doc_name}.json"));
    if pub_idx.exists() {
        std::fs::create_dir_all(priv_idx.parent().unwrap())?;
        std::fs::rename(pub_idx, priv_idx)?;
    }
    Ok(())
}

fn apply_plan(
    layout: &WikiLayout,
    scope: WikiScope,
    doc_name: &str,
    summary: &str,
    client: &dyn LlmClient,
    system: &str,
    entity_types: &[String],
    concept_slugs: &mut Vec<String>,
    entity_slugs: &mut Vec<String>,
) -> Result<()> {
    let p = wiki::wiki_link_prefix(scope);
    let concept_briefs = wiki::briefs_for_dir(&layout.concepts, &format!("{p}concepts"))?;
    let entity_briefs = wiki::briefs_for_dir(&layout.entities, &format!("{p}entities"))?;
    let plan_user = prompts::concepts_plan_user(&concept_briefs, &entity_briefs, entity_types);
    let plan = match client.complete_json(
        system,
        &format!("Document summary:\n{summary}\n\n{plan_user}"),
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("llm: concept plan JSON failed ({e:#}); continuing with empty plan");
            crate::llm::client::empty_wiki_plan()
        }
    };

    if let Some(items) = plan["concepts"]["create"].as_array() {
        for item in items {
            let name = wiki::slugify(item["name"].as_str().unwrap_or("concept"));
            let title = item["title"].as_str().unwrap_or(&name);
            write_concept(layout, doc_name, &name, title, summary, client, system, false)?;
            concept_slugs.push(name);
        }
    }
    if let Some(items) = plan["concepts"]["update"].as_array() {
        for item in items {
            let name = wiki::slugify(item["name"].as_str().unwrap_or("concept"));
            let title = item["title"].as_str().unwrap_or(&name);
            write_concept(layout, doc_name, &name, title, summary, client, system, true)?;
            if !concept_slugs.contains(&name) {
                concept_slugs.push(name);
            }
        }
    }
    if let Some(related) = plan["concepts"]["related"].as_array() {
        for slug in related {
            if let Some(s) = slug.as_str() {
                let name = wiki::slugify(s);
                wiki::add_related_link(layout, scope, &name, doc_name)?;
                if !concept_slugs.contains(&name) {
                    concept_slugs.push(name);
                }
            }
        }
    }

    if let Some(items) = plan["entities"]["create"].as_array() {
        for item in items {
            let name = wiki::slugify(item["name"].as_str().unwrap_or("entity"));
            let title = item["title"].as_str().unwrap_or(&name);
            let etype = item["type"].as_str().unwrap_or("product");
            write_entity(layout, doc_name, &name, title, etype, summary, client, system, false)?;
            entity_slugs.push(name);
        }
    }
    if let Some(items) = plan["entities"]["update"].as_array() {
        for item in items {
            let name = wiki::slugify(item["name"].as_str().unwrap_or("entity"));
            let title = item["title"].as_str().unwrap_or(&name);
            let etype = item["type"].as_str().unwrap_or("product");
            write_entity(layout, doc_name, &name, title, etype, summary, client, system, true)?;
            if !entity_slugs.contains(&name) {
                entity_slugs.push(name);
            }
        }
    }
    if let Some(related) = plan["entities"]["related"].as_array() {
        for slug in related {
            if let Some(s) = slug.as_str() {
                let name = wiki::slugify(s);
                wiki::add_related_link(layout, scope, &name, doc_name)?;
                if !entity_slugs.contains(&name) {
                    entity_slugs.push(name);
                }
            }
        }
    }

    Ok(())
}

fn write_concept(
    layout: &WikiLayout,
    doc_name: &str,
    name: &str,
    title: &str,
    summary: &str,
    client: &dyn LlmClient,
    system: &str,
    update: bool,
) -> Result<()> {
    let path = wiki::concept_path(&layout.concepts, name);
    let user = if update && path.exists() {
        let existing = wiki::read_page(&path)?;
        prompts::concept_update_user(doc_name, title, &existing)
    } else {
        prompts::concept_page_user(doc_name, title, update)
    };
    let page = client.complete_json(
        system,
        &format!("Summary context:\n{summary}\n\n{user}"),
    )?;
    let body = page["content"].as_str().unwrap_or("");
    let desc = page["description"].as_str().unwrap_or(title);
    wiki::write_page(&path, &wiki::wrap_page_frontmatter("Concept", title, desc, body))
}

fn write_entity(
    layout: &WikiLayout,
    doc_name: &str,
    name: &str,
    title: &str,
    entity_type: &str,
    summary: &str,
    client: &dyn LlmClient,
    system: &str,
    update: bool,
) -> Result<()> {
    let path = wiki::entity_path(&layout.entities, name);
    let user = if update && path.exists() {
        let existing = wiki::read_page(&path)?;
        prompts::entity_update_user(doc_name, title, entity_type, &existing)
    } else {
        prompts::entity_page_user(doc_name, title, entity_type)
    };
    let page = client.complete_json(
        system,
        &format!("Summary context:\n{summary}\n\n{user}"),
    )?;
    let body = page["content"].as_str().unwrap_or("");
    let desc = page["description"].as_str().unwrap_or(title);
    wiki::write_page(
        &path,
        &wiki::wrap_page_frontmatter(&wiki::entity_okf_type(entity_type), title, desc, body),
    )
}

pub fn compile_long_doc(
    kb: &KbPaths,
    doc_name: &str,
    raw_pdf: &Path,
    original_path: &Path,
    path_private: bool,
) -> Result<CompileOutcome> {
    let scope = if path_private {
        WikiScope::Private
    } else {
        WikiScope::Public
    };
    let doc = index::build_page_index(kb, doc_name, raw_pdf, scope)?;
    let overview = index::tree_overview_markdown(&doc);
    compile_from_content(kb, doc_name, &overview, original_path, true, path_private)
}
