use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_ollama_host")]
    pub ollama_host: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    /// Vision-capable Ollama model for `image = true` ingest. Falls back to `ollama_model`.
    #[serde(default)]
    pub vision_model: Option<String>,
    #[serde(default = "default_n_ctx")]
    pub n_ctx: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_n_ctx() -> u32 {
    32768
}
fn default_temperature() -> f32 {
    0.3
}
fn default_ollama_host() -> String {
    "http://127.0.0.1:11434".into()
}
fn default_ollama_model() -> String {
    "batiai/gemma4-e2b:q4".into()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            ollama_host: default_ollama_host(),
            ollama_model: default_ollama_model(),
            vision_model: None,
            n_ctx: default_n_ctx(),
            temperature: default_temperature(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_pageindex_threshold")]
    pub pageindex_threshold: u32,
    #[serde(default = "default_entity_types")]
    pub entity_types: Vec<String>,
    /// Directories walked by `scan` and `watch` (`~` expands to $HOME).
    #[serde(default = "default_scan_paths", alias = "scan_roots")]
    pub scan_paths: Vec<String>,
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// Index image files via multimodal LLM (default: off).
    #[serde(default)]
    pub image: bool,
    /// Index video/audio metadata stubs (default: off).
    #[serde(default = "default_index_media")]
    pub index_media: bool,
    /// Index software user-profile paths: .config, saves, mods, userdata (default: off).
    #[serde(default, alias = "index_binary_media")]
    pub index_user_profiles: bool,
}

fn default_index_media() -> bool {
    false
}

fn default_language() -> String {
    "en".into()
}
fn default_pageindex_threshold() -> u32 {
    20
}
fn default_entity_types() -> Vec<String> {
    vec![
        "person".into(),
        "organization".into(),
        "place".into(),
        "product".into(),
    ]
}
fn default_scan_paths() -> Vec<String> {
    vec!["~".into(), "/media".into(), "/mnt".into()]
}
fn default_max_file_size_mb() -> u64 {
    500
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            language: default_language(),
            pageindex_threshold: default_pageindex_threshold(),
            entity_types: default_entity_types(),
            scan_paths: default_scan_paths(),
            max_file_size_mb: default_max_file_size_mb(),
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
            image: false,
            index_media: false,
            index_user_profiles: false,
        }
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        Self::expand_path("~/.everythingkb/config.toml")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        toml::from_str(&raw).context("parse config.toml")
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self).context("serialize config")?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Write defaults only when no config exists yet (migrates legacy path first).
    pub fn save_if_missing(&self, path: &Path, legacy: PathBuf) -> Result<()> {
        if path.exists() {
            return Ok(());
        }
        if legacy.exists() {
            return Self::load(&legacy)?.save(path);
        }
        self.save(path)
    }

    pub fn expand_path(s: &str) -> PathBuf {
        if s.starts_with("~/") || s == "~" {
            if let Ok(home) = std::env::var("HOME") {
                if s == "~" {
                    return PathBuf::from(home);
                }
                return PathBuf::from(home).join(&s[2..]);
            }
        }
        PathBuf::from(s)
    }

    pub fn resolved_scan_paths(&self) -> Vec<PathBuf> {
        self.scan_paths.iter().map(|s| Self::expand_path(s)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde() {
        let p = Config::expand_path("~/foo");
        assert!(p.ends_with("foo"));
    }

    #[test]
    fn default_config_path() {
        let p = Config::default_path();
        assert!(p.ends_with(".everythingkb/config.toml"));
    }

    #[test]
    fn scan_roots_alias_loads_as_scan_paths() {
        let c: Config = toml::from_str(r#"scan_roots = ["/tmp", "~/notes"]"#).unwrap();
        assert_eq!(c.scan_paths, vec!["/tmp", "~/notes"]);
    }
}
