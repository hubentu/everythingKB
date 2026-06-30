use std::path::Path;

use crate::config::Config;

/// True when the file path falls under a configured `private_paths` root.
pub fn is_private_path(path: &Path, config: &Config) -> bool {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    config.resolved_private_paths().iter().any(|root| {
        let root = root.canonicalize().unwrap_or_else(|_| root.clone());
        resolved.starts_with(&root)
    })
}

/// Path rule or LLM summary flag.
pub fn doc_is_private(path_private: bool, summary_json: &serde_json::Value, detect: bool) -> bool {
    if path_private {
        return true;
    }
    if !detect {
        return false;
    }
    summary_json["private"].as_bool().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_path_prefix() {
        let mut config = Config::default();
        config.private_paths = vec!["/tmp/private-zone".into()];
        let p = Path::new("/tmp/private-zone/medical/report.pdf");
        std::fs::create_dir_all("/tmp/private-zone/medical").ok();
        std::fs::write(p, b"x").ok();
        assert!(is_private_path(p, &config));
        let _ = std::fs::remove_file(p);
    }
}
