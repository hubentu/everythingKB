use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::registry::Registry;

const AGENTS_MD: &str = r#"# Wiki Agent Instructions

This knowledge base follows the [Open Knowledge Format (OKF) v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md):
- `summaries/` — document summaries (`type: Document Summary`, `resource` = original file)
- `concepts/` — abstract ideas (`type: Concept`)
- `entities/` — named things (`type: Person`, `Organization`, etc.)
- `sources/` — converted source markdown
- Cross-links use standard markdown: `[title](concepts/slug.md)`
- Bundle root `index.md` declares `okf_version: "0.1"`; `log.md` records ingest history
"#;

const INDEX_MD: &str = r#"---
okf_version: "0.1"
---

# Knowledge Base Index

Documents indexed by everythingKB.
"#;

/// Public or private wiki content directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiScope {
    Public,
    Private,
}

impl WikiScope {
    pub fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

/// Paths for one wiki zone (public or private).
#[derive(Debug, Clone)]
pub struct WikiLayout {
    pub wiki: PathBuf,
    pub summaries: PathBuf,
    pub concepts: PathBuf,
    pub entities: PathBuf,
    pub sources: PathBuf,
}

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
    pub private_wiki: PathBuf,
    pub pageindex: PathBuf,
}

impl KbPaths {
    pub fn new(root: PathBuf) -> Self {
        let meta = root.join(".everythingkb");
        let wiki = root.join("wiki");
        let private_wiki = wiki.join("private");
        Self {
            config_path: Config::default_path(),
            registry_path: meta.join("registry.db"),
            summaries: wiki.join("summaries"),
            concepts: wiki.join("concepts"),
            entities: wiki.join("entities"),
            sources: wiki.join("sources"),
            private_wiki: private_wiki.clone(),
            wiki,
            pageindex: meta.join("pageindex"),
            root,
            meta,
        }
    }

    pub fn layout(&self, scope: WikiScope) -> WikiLayout {
        match scope {
            WikiScope::Public => WikiLayout {
                wiki: self.wiki.clone(),
                summaries: self.summaries.clone(),
                concepts: self.concepts.clone(),
                entities: self.entities.clone(),
                sources: self.sources.clone(),
            },
            WikiScope::Private => WikiLayout {
                wiki: self.private_wiki.clone(),
                summaries: self.private_wiki.join("summaries"),
                concepts: self.private_wiki.join("concepts"),
                entities: self.private_wiki.join("entities"),
                sources: self.private_wiki.join("sources"),
            },
        }
    }

    pub fn pageindex_dir(&self, scope: WikiScope) -> PathBuf {
        match scope {
            WikiScope::Public => self.pageindex.clone(),
            WikiScope::Private => self.pageindex.join("private"),
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
        let private = paths.layout(WikiScope::Private);

        for dir in [
            &paths.meta,
            &paths.wiki,
            &paths.summaries,
            &paths.concepts,
            &paths.entities,
            &paths.sources,
            &private.wiki,
            &private.summaries,
            &private.concepts,
            &private.entities,
            &private.sources,
            &paths.pageindex,
            &paths.pageindex_dir(WikiScope::Private),
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
        let private_index = private.wiki.join("index.md");
        if !private_index.exists() {
            std::fs::write(
                &private_index,
                "---\nokf_version: \"0.1\"\n---\n\n# Private Knowledge Base Index\n\nSensitive / personal documents.\n",
            )?;
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
        if !p.exists() {
            return 0;
        }
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
