use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::registry::Registry;

const AGENTS_MD: &str = r#"# Wiki Agent Instructions

This knowledge base follows OpenKB conventions:
- Summaries in `summaries/`
- Concepts in `concepts/` with [[wikilinks]]
- Entities in `entities/`
- Source material in `sources/`
"#;

const INDEX_MD: &str = r#"# Knowledge Base Index

Documents indexed by everythingKB.
"#;

#[derive(Debug, Clone)]
pub struct KbPaths {
    pub root: PathBuf,
    pub meta: PathBuf,
    pub config_path: PathBuf,
    pub registry_path: PathBuf,
    pub wiki: PathBuf,
    pub summaries: PathBuf,
    pub concepts: PathBuf,
    pub entities: PathBuf,
    pub sources: PathBuf,
    pub pageindex: PathBuf,
}

impl KbPaths {
    pub fn new(root: PathBuf) -> Self {
        let meta = root.join(".everythingkb");
        Self {
            config_path: Config::default_path(),
            registry_path: meta.join("registry.db"),
            wiki: root.join("wiki"),
            summaries: root.join("wiki/summaries"),
            concepts: root.join("wiki/concepts"),
            entities: root.join("wiki/entities"),
            sources: root.join("wiki/sources"),
            pageindex: meta.join("pageindex"),
            root,
            meta,
        }
    }

    pub fn default_location() -> PathBuf {
        Config::expand_path("~/.everythingkb/kb")
    }

    pub fn open(root: Option<PathBuf>) -> Result<Self> {
        let kb = root.unwrap_or_else(Self::default_location);
        let paths = Self::new(kb.clone());
        if !paths.registry_path.exists() {
            anyhow::bail!(
                "KB not initialized at {}. Run `everythingkb init` first.",
                kb.display()
            );
        }
        Ok(paths)
    }

    fn legacy_config_path(&self) -> PathBuf {
        self.meta.join("config.toml")
    }

    fn ensure_config(&self) -> Result<()> {
        if self.config_path.exists() {
            return Ok(());
        }
        let legacy = self.legacy_config_path();
        if legacy.exists() {
            let config = Config::load(&legacy)?;
            config.save(&self.config_path)?;
        }
        Ok(())
    }

    pub fn init(root: Option<PathBuf>, config: &Config) -> Result<Self> {
        let kb = root.unwrap_or_else(Self::default_location);
        let paths = Self::new(kb);

        for dir in [
            &paths.meta,
            &paths.wiki,
            &paths.summaries,
            &paths.concepts,
            &paths.entities,
            &paths.sources,
            &paths.pageindex,
            &paths.sources.join("images"),
        ] {
            std::fs::create_dir_all(dir)?;
        }

        let agents = paths.wiki.join("AGENTS.md");
        if !agents.exists() {
            std::fs::write(&agents, AGENTS_MD)?;
        }
        let index = paths.wiki.join("index.md");
        if !index.exists() {
            std::fs::write(&index, INDEX_MD)?;
        }
        let log = paths.wiki.join("log.md");
        if !log.exists() {
            std::fs::write(&log, "# Ingest Log\n\n")?;
        }

        config.save_if_missing(&paths.config_path, paths.legacy_config_path())?;
        Registry::open(&paths.registry_path)?;

        Ok(paths)
    }

    pub fn load_config(&self) -> Result<Config> {
        self.ensure_config()?;
        if self.config_path.exists() {
            return Config::load(&self.config_path);
        }
        anyhow::bail!(
            "Config not found at {}. Run `everythingkb init` first.",
            self.config_path.display()
        )
    }

    pub fn open_registry(&self) -> Result<Registry> {
        Registry::open(&self.registry_path)
    }
}

pub fn wiki_stats(wiki: &Path) -> Result<(usize, usize, usize)> {
    let count = |p: &Path| -> usize {
        walkdir::WalkDir::new(p)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count()
    };
    Ok((
        count(&wiki.join("summaries")),
        count(&wiki.join("concepts")),
        count(&wiki.join("entities")),
    ))
}
