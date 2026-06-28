use std::path::Path;

use regex::Regex;

/// Default path segments to skip during scanning.
const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    ".cache",
    "node_modules",
    "__pycache__",
    ".local/share/Trash",
    "steamapps/common",
    ".steam",
    "/proc",
    "/sys",
    "/dev",
    "/run",
];

/// User-data patterns that override exclusions (app configs, saves, mods).
const DEFAULT_INCLUDES: &[&str] = &[
    "/Saves/",
    "/save/",
    "/Save/",
    "/userdata/",
    "/.config/",
    "/Mods/",
    "/mods/",
    ".sav",
    ".save",
];

pub struct ExclusionEngine {
    exclude_res: Vec<Regex>,
    include_res: Vec<Regex>,
    max_bytes: u64,
}

impl ExclusionEngine {
    pub fn new(config_excludes: &[String], config_includes: &[String], max_file_size_mb: u64) -> Self {
        let mut exclude_patterns: Vec<String> = DEFAULT_EXCLUDES
            .iter()
            .map(|s| regex::escape(s))
            .collect();
        exclude_patterns.extend(config_excludes.iter().cloned());

        let mut include_patterns: Vec<String> = DEFAULT_INCLUDES
            .iter()
            .map(|s| regex::escape(s))
            .collect();
        include_patterns.extend(config_includes.iter().cloned());

        Self {
            exclude_res: compile_patterns(&exclude_patterns),
            include_res: compile_patterns(&include_patterns),
            max_bytes: max_file_size_mb.saturating_mul(1024 * 1024),
        }
    }

    pub fn should_skip(&self, path: &Path) -> bool {
        let s = path.to_string_lossy();

        if self.include_res.iter().any(|re| re.is_match(&s)) {
            return false;
        }

        if self.exclude_res.iter().any(|re| re.is_match(&s)) {
            return true;
        }

        if path.is_file() {
            if let Ok(meta) = path.metadata() {
                if meta.len() > self.max_bytes {
                    return true;
                }
            }
        }

        false
    }
}

fn compile_patterns(patterns: &[String]) -> Vec<Regex> {
    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_node_modules() {
        let engine = ExclusionEngine::new(&[], &[], 500);
        assert!(engine.should_skip(Path::new("/home/u/project/node_modules/pkg/index.js")));
    }

    #[test]
    fn includes_steam_userdata() {
        let engine = ExclusionEngine::new(&[], &[], 500);
        let p = Path::new("/home/u/.steam/steam/userdata/123/456/remote/save.sav");
        assert!(!engine.should_skip(p));
    }
}
