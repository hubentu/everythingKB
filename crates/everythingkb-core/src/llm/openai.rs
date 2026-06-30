use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::json;

use crate::config::Config;
use crate::llm::client::{parse_json_response, LlmClient};

static HTTP: OnceLock<Client> = OnceLock::new();

pub struct OpenAiClient {
    base_url: String,
    api_key: String,
    model: String,
    temperature: f32,
    max_tokens: u32,
    disable_thinking: bool,
    timeout: Duration,
}

impl OpenAiClient {
    pub fn new(config: &Config) -> Result<Self> {
        let base_url = config
            .llm
            .openai_base_url
            .clone()
            .filter(|s| !s.is_empty())
            .context(
                "llm.openai_base_url is required when llm.backend = \"openai\" \
                 (e.g. http://192.168.1.167:8000/v1 for vLLM)",
            )?;
        let model = config
            .llm
            .openai_model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| config.llm.ollama_model.clone());
        let api_key = config
            .llm
            .openai_api_key
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "not-needed".into());
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            temperature: config.llm.temperature,
            max_tokens: config.llm.n_ctx.min(4096),
            disable_thinking: config.llm.openai_disable_thinking,
            timeout: Duration::from_secs(config.llm.openai_timeout_secs),
        })
    }

    fn http(&self) -> Client {
        HTTP.get_or_init(|| {
            Client::builder()
                .build()
                .expect("reqwest client")
        })
        .clone()
    }

    fn chat(&self, system: &str, user: &str, json: bool) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
        });
        if json {
            body["response_format"] = json!({"type": "json_object"});
        }
        if self.disable_thinking {
            body["chat_template_kwargs"] = json!({"enable_thinking": false});
        }

        let resp = self
            .http()
            .post(&url)
            .timeout(self.timeout)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .with_context(|| format!("POST {url}"))?;

        let status = resp.status();
        let text = resp.text()?;
        if !status.is_success() {
            anyhow::bail!("OpenAI-compatible API {status}: {text}");
        }

        let v: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("parse response from {url}"))?;
        message_content(&v).context("missing assistant content in API response")
    }
}

/// vLLM thinking models may leave `content` null and fill `reasoning` instead.
fn message_content(v: &serde_json::Value) -> Option<String> {
    let msg = &v["choices"][0]["message"];
    if let Some(c) = msg["content"].as_str() {
        if !c.trim().is_empty() {
            return Some(c.to_string());
        }
    }
    msg["reasoning"].as_str().map(String::from)
}

impl LlmClient for OpenAiClient {
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.chat(system, user, false)
    }

    fn complete_json(&self, system: &str, user: &str) -> Result<serde_json::Value> {
        let raw = self.chat(system, user, true)?;
        parse_json_response(system, user, &raw, |sys, usr| self.chat(sys, usr, true))
    }

    fn complete_image(&self, _system: &str, _user: &str, _image_path: &Path) -> Result<String> {
        anyhow::bail!("vision/image ingest requires llm.backend = \"ollama\" with a vision model")
    }
}
