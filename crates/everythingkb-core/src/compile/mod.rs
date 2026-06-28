pub mod prompts;

use std::path::Path;

use anyhow::Result;

use crate::index;
use crate::kb::KbPaths;
use crate::llm::{build_client, LlmClient};
use crate::wiki;

pub struct CompileOutcome {
    pub doc_name: String,
    pub summary_path: std::path::PathBuf,
}

pub fn compile_short_doc(
    kb: &KbPaths,
    doc_name: &str,
    source_path: &Path,
    original_path: &Path,
) -> Result<CompileOutcome> {
    let content = wiki::read_page(source_path)?;
    compile_from_content(kb, doc_name, &content, original_path, false)
}

fn compile_from_content(
    kb: &KbPaths,
    doc_name: &str,
    content: &str,
    original_path: &Path,
    is_long: bool,
) -> Result<CompileOutcome> {
    let config = kb.load_config()?;
    let client = build_client(&config)?;
    let language = &config.language;
    let schema = wiki::agents_md(&kb.wiki);
    let system = prompts::system_schema(language, &schema);

    let summary_user = prompts::summary_user(doc_name, content);
    let summary_json = client.complete_json(&system, &summary_user)?;
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

    apply_plan(
        kb,
        doc_name,
        &summary_body,
        &*client,
        &system,
        &config.entity_types,
        &mut concept_slugs,
        &mut entity_slugs,
    )?;

    let targets = wiki::known_targets(kb)?;
    let whitelist = prompts::known_targets_user(&targets);
    let _ = client.complete(&system, &whitelist);

    let final_summary = client
        .complete(
            &system,
            &format!(
                "Document summary draft:\n{summary_body}\n\n{}",
                prompts::summary_rewrite_user()
            ),
        )
        .unwrap_or(summary_body.clone());

    let summary_path = wiki::summary_path(&kb.summaries, doc_name);
    let source = wiki::source_path_display(original_path);
    let summary_md = wiki::wrap_summary_frontmatter(
        doc_name,
        &source,
        &description,
        &final_summary,
    );
    wiki::write_page(&summary_path, &summary_md)?;

    wiki::backlink_summary(&kb.summaries, doc_name, &concept_slugs)?;
    wiki::update_index(
        &kb.wiki,
        doc_name,
        &concept_slugs,
        &entity_slugs,
        &description,
    )?;

    let action = if is_long { "compile-long" } else { "compile" };
    wiki::append_log(&kb.wiki, action, doc_name)?;
    Ok(CompileOutcome {
        doc_name: doc_name.into(),
        summary_path,
    })
}

fn apply_plan(
    kb: &KbPaths,
    doc_name: &str,
    summary: &str,
    client: &dyn LlmClient,
    system: &str,
    entity_types: &[String],
    concept_slugs: &mut Vec<String>,
    entity_slugs: &mut Vec<String>,
) -> Result<()> {
    let concept_briefs = wiki::briefs_for_dir(&kb.concepts, "concepts")?;
    let entity_briefs = wiki::briefs_for_dir(&kb.entities, "entities")?;
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
            write_concept(kb, doc_name, &name, title, summary, client, system, false)?;
            concept_slugs.push(name);
        }
    }
    if let Some(items) = plan["concepts"]["update"].as_array() {
        for item in items {
            let name = wiki::slugify(item["name"].as_str().unwrap_or("concept"));
            let title = item["title"].as_str().unwrap_or(&name);
            write_concept(kb, doc_name, &name, title, summary, client, system, true)?;
            if !concept_slugs.contains(&name) {
                concept_slugs.push(name);
            }
        }
    }
    if let Some(related) = plan["concepts"]["related"].as_array() {
        for slug in related {
            if let Some(s) = slug.as_str() {
                let name = wiki::slugify(s);
                wiki::add_related_link(&kb.concepts, &name, doc_name)?;
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
            write_entity(kb, doc_name, &name, title, etype, summary, client, system, false)?;
            entity_slugs.push(name);
        }
    }
    if let Some(items) = plan["entities"]["update"].as_array() {
        for item in items {
            let name = wiki::slugify(item["name"].as_str().unwrap_or("entity"));
            let title = item["title"].as_str().unwrap_or(&name);
            let etype = item["type"].as_str().unwrap_or("product");
            write_entity(kb, doc_name, &name, title, etype, summary, client, system, true)?;
            if !entity_slugs.contains(&name) {
                entity_slugs.push(name);
            }
        }
    }
    if let Some(related) = plan["entities"]["related"].as_array() {
        for slug in related {
            if let Some(s) = slug.as_str() {
                let name = wiki::slugify(s);
                wiki::add_related_link(&kb.entities, &name, doc_name)?;
                if !entity_slugs.contains(&name) {
                    entity_slugs.push(name);
                }
            }
        }
    }

    Ok(())
}

fn write_concept(
    kb: &KbPaths,
    doc_name: &str,
    name: &str,
    title: &str,
    summary: &str,
    client: &dyn LlmClient,
    system: &str,
    update: bool,
) -> Result<()> {
    let path = wiki::concept_path(&kb.concepts, name);
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
    wiki::write_page(&path, &wiki::wrap_page_frontmatter(title, desc, body))
}

fn write_entity(
    kb: &KbPaths,
    doc_name: &str,
    name: &str,
    title: &str,
    entity_type: &str,
    summary: &str,
    client: &dyn LlmClient,
    system: &str,
    update: bool,
) -> Result<()> {
    let path = wiki::entity_path(&kb.entities, name);
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
    wiki::write_page(&path, &wiki::wrap_page_frontmatter(title, desc, body))
}

pub fn compile_long_doc(
    kb: &KbPaths,
    doc_name: &str,
    raw_pdf: &Path,
    original_path: &Path,
) -> Result<CompileOutcome> {
    let doc = index::build_page_index(kb, doc_name, raw_pdf)?;
    let overview = index::tree_overview_markdown(&doc);
    compile_from_content(kb, doc_name, &overview, original_path, true)
}
