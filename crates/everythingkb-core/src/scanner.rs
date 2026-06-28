use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use jwalk::WalkDir;

use crate::classifier;
use crate::config::Config;
use crate::exclusions::ExclusionEngine;

#[derive(Debug, Clone)]
pub struct ScanHit {
    pub path: PathBuf,
}

/// Walk scan roots and invoke `f` for each indexable file (streaming — no full-tree buffer).
pub fn for_each_hit(config: &Config, mut f: impl FnMut(ScanHit) -> Result<()>) -> Result<()> {
    let engine = ExclusionEngine::new(
        &config.exclude_patterns,
        &config.include_patterns,
        config.max_file_size_mb,
    );
    let mut seen = HashSet::new();

    for root in config.resolved_scan_paths() {
        if !root.exists() {
            continue;
        }
        eprintln!("Walking {}...", root.display());
        walk_root(&root, &engine, config, root_is_hidden(&root), &mut seen, &mut f)?;
    }
    Ok(())
}

fn walk_root(
    root: &Path,
    engine: &ExclusionEngine,
    config: &Config,
    allow_hidden: bool,
    seen: &mut HashSet<PathBuf>,
    f: &mut impl FnMut(ScanHit) -> Result<()>,
) -> Result<()> {
    for entry in WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(!allow_hidden)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path().to_path_buf();
        if engine.should_skip(&path) {
            continue;
        }
        if !path.is_file() || !classifier::should_scan_file(&path, config) {
            continue;
        }
        if !seen.insert(path.clone()) {
            continue;
        }
        f(ScanHit { path })?;
    }
    Ok(())
}

/// True when the scan root itself lives under a hidden path (e.g. `~/.notes`).
fn root_is_hidden(root: &Path) -> bool {
    use std::path::Component;
    root.components().any(|c| {
        matches!(c, Component::Normal(name) if name.to_str().is_some_and(|n| n.starts_with('.')))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_root_detection() {
        assert!(!root_is_hidden(Path::new("/home/u/Documents")));
        assert!(root_is_hidden(Path::new("/home/u/.config/myapp")));
    }
}
