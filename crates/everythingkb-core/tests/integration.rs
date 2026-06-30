use everythingkb_core::classifier::{classify, FileKind};
use everythingkb_core::config::Config;
use everythingkb_core::exclusions::ExclusionEngine;
use everythingkb_core::wiki;

#[test]
fn config_defaults_ollama() {
    let c = Config::default();
    assert_eq!(c.llm.ollama_model, "batiai/gemma4-e2b:q4");
}

#[test]
fn classifier_markdown() {
    let dir = std::env::temp_dir().join("everythingkb_it");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("note.md");
    std::fs::write(&path, "# hello").unwrap();
    assert_eq!(classify(&path, &Config::default()), FileKind::Document);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn summary_frontmatter_okf() {
    let md = wiki::wrap_summary_frontmatter(
        "my-doc",
        "/home/u/Documents/notes/paper.pdf",
        "One sentence summary.",
        "body",
        false,
    );
    assert!(md.contains("type: Document Summary"));
    assert!(md.contains("resource: /home/u/Documents/notes/paper.pdf"));
    assert!(md.contains("description: One sentence summary."));
    assert!(md.contains("timestamp:"));
    let private = wiki::wrap_summary_frontmatter("x", "/p", "d", "b", true);
    assert!(private.contains("tags: [private]"));
    assert!(private.contains("private: true"));
}

#[test]
fn exclusions_skip_git() {
    let engine = ExclusionEngine::new(&[], &[], 500);
    assert!(engine.should_skip(std::path::Path::new("/home/user/project/.git/config")));
}
