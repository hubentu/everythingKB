use anyhow::{Context, Result};

pub trait LlmClient: Send + Sync {
    fn complete(&self, system: &str, user: &str) -> Result<String>;

    fn complete_json(&self, system: &str, user: &str) -> Result<serde_json::Value> {
        let raw = self.complete(system, user)?;
        parse_json_response(system, user, &raw, |sys, usr| self.complete(sys, usr))
    }

    /// Multimodal completion with one image (Ollama vision models).
    fn complete_image(&self, system: &str, user: &str, image_path: &std::path::Path) -> Result<String> {
        let _ = (system, user, image_path);
        anyhow::bail!("vision LLM not supported by this client")
    }
}

pub(crate) fn parse_json_response(
    system: &str,
    user: &str,
    raw: &str,
    retry: impl FnOnce(&str, &str) -> Result<String>,
) -> Result<serde_json::Value> {
    if let Ok(v) = extract_json(raw) {
        return Ok(v);
    }
    if let Some(v) = recover_page_json(raw.trim()) {
        return Ok(v);
    }
    if let Some(v) = prose_as_page_json(raw) {
        return Ok(v);
    }

    let retry_user = format!(
        "{user}\n\nYour previous reply was not valid JSON. \
         Reply with ONLY one JSON object. No markdown fences, no commentary."
    );
    let raw2 = retry(system, &retry_user)?;
    if let Ok(v) = extract_json(&raw2) {
        return Ok(v);
    }
    if let Some(v) = recover_page_json(raw2.trim()) {
        return Ok(v);
    }
    if let Some(v) = prose_as_page_json(&raw2) {
        return Ok(v);
    }

    eprintln!(
        "llm: JSON parse failed; model output starts with: {}",
        raw2.chars().take(200).collect::<String>()
    );
    extract_json(&raw2).context("parse JSON from model output")
}

/// Pull JSON out of LLM output (fences, thinking blocks, prose wrappers).
pub fn extract_json(raw: &str) -> Result<serde_json::Value> {
    let mut s = raw.trim();

    for marker in [
        "<start_of_turn>model\n",
        "<start_of_turn>model",
        "model\n",
    ] {
        if let Some(rest) = s.strip_prefix(marker) {
            s = rest.trim();
        }
    }

    if let Some(rest) = s.strip_prefix("```json") {
        s = rest.trim_end_matches("```").trim();
    } else if let Some(rest) = s.strip_prefix("```") {
        s = rest.trim_end_matches("```").trim();
    }

    const THINK_CLOSE: &str = concat!("</", "think", ">");
    if let Some(idx) = s.find(THINK_CLOSE) {
        s = s[idx + THINK_CLOSE.len()..].trim();
    }

    if let Ok(v) = serde_json::from_str(s) {
        return Ok(v);
    }

    let start = s.find('{').context("no JSON object in model output")?;
    let slice = &s[start..];
    if let Some(end) = slice.rfind('}') {
        if let Ok(v) = serde_json::from_str(&slice[..=end]) {
            return Ok(v);
        }
    }
    if let Some(v) = recover_page_json(slice) {
        return Ok(v);
    }

    anyhow::bail!("invalid JSON object in model output")
}

/// Salvage `{description, content}` when Ollama truncates mid-string (no closing `}`).
fn recover_page_json(s: &str) -> Option<serde_json::Value> {
    let desc = json_field_string(s, "description")?;
    let content = json_field_string(s, "content").unwrap_or_default();
    if desc.is_empty() && content.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "description": desc,
        "content": content
    }))
}

fn json_field_string(s: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let rest = s.split_once(&marker)?.1.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    parse_json_string_value(rest.strip_prefix('"')?)
}

fn parse_json_string_value(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let esc = chars.next()?;
                out.push(match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    other => other,
                });
            }
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out) // truncated — no closing quote
    }
}

/// Models often return markdown when asked for `{description, content}` JSON.
fn prose_as_page_json(raw: &str) -> Option<serde_json::Value> {
    let s = raw.trim();
    if s.is_empty() || s.starts_with('{') {
        return None;
    }
    let desc = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(100)
        .collect::<String>();
    Some(serde_json::json!({
        "description": desc,
        "content": s
    }))
}

pub fn empty_wiki_plan() -> serde_json::Value {
    serde_json::json!({
        "concepts": { "create": [], "update": [], "related": [] },
        "entities": { "create": [], "update": [], "related": [] }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_fallback_wraps_markdown() {
        let v = prose_as_page_json("# Title\n\nBody text.").unwrap();
        assert_eq!(v["content"], "# Title\n\nBody text.");
        assert!(v["description"].as_str().unwrap().contains("Title"));
    }

    #[test]
    fn extract_json_from_fences() {
        let v = extract_json("```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn recover_truncated_summary_json() {
        let raw = r##"{
"description": "Enrollment process overview.",
"content": "# Enrollment Appl"##;
        let v = extract_json(raw).unwrap();
        assert_eq!(v["description"], "Enrollment process overview.");
        assert!(v["content"].as_str().unwrap().starts_with("# Enrollment"));
    }
}
