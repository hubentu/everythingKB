use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::llm::client::{parse_json_response, LlmClient};

pub struct OllamaClient {
    model: String,
    vision_model: String,
    host: String,
    temperature: f32,
    n_ctx: u32,
}

impl OllamaClient {
    pub fn new(config: &Config) -> Result<Self> {
        let model = config.llm.ollama_model.clone();
        let vision_model = config
            .llm
            .vision_model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| model.clone());
        Ok(Self {
            model,
            vision_model,
            host: config.llm.ollama_host.clone(),
            temperature: config.llm.temperature,
            n_ctx: config.llm.n_ctx,
        })
    }

    fn generate(
        &self,
        model: &str,
        system: &str,
        user: &str,
        json: bool,
        image_path: Option<&Path>,
    ) -> Result<String> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use ollama_rs::generation::completion::request::GenerationRequest;
        use ollama_rs::generation::images::Image;
        use ollama_rs::generation::parameters::{FormatType, ThinkType};
        use ollama_rs::models::ModelOptions;
        use ollama_rs::Ollama;

        let ollama = Ollama::try_new(&self.host)
            .map_err(|e| anyhow::anyhow!("invalid llm.ollama_host `{}`: {e}", self.host))?;

        let mut req = GenerationRequest::new(model.to_string(), user.to_string())
            .system(system.to_string())
            .think(ThinkType::False)
            .options(
                ModelOptions::default()
                    .temperature(self.temperature)
                    .num_ctx(self.n_ctx as u64),
            );

        if let Some(path) = image_path {
            let bytes = std::fs::read(path)
                .with_context(|| format!("read image {}", path.display()))?;
            req = req.add_image(Image::from_base64(STANDARD.encode(bytes)));
        }

        if json {
            req = req.format(FormatType::Json);
        }

        let rt = tokio::runtime::Runtime::new()?;
        let resp = rt.block_on(async { ollama.generate(req).await }).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("unable to load model") {
                anyhow::anyhow!(
                    "Ollama failed to load model `{model}`. Try `ollama run {model} hi` to verify, \
                     update Ollama from https://ollama.com/download, or set llm.ollama_model / \
                     llm.vision_model to a working model in ~/.everythingkb/config.toml. \
                     Original: {msg}",
                )
            } else {
                anyhow::anyhow!("Ollama request failed: {msg}")
            }
        })?;
        Ok(resp.response)
    }
}

impl LlmClient for OllamaClient {
    fn complete(&self, system: &str, user: &str) -> Result<String> {
        self.generate(&self.model, system, user, false, None)
    }

    fn complete_json(&self, system: &str, user: &str) -> Result<serde_json::Value> {
        let raw = self.generate(&self.model, system, user, true, None)?;
        parse_json_response(system, user, &raw, |sys, usr| {
            self.generate(&self.model, sys, usr, true, None)
        })
    }

    fn complete_image(&self, system: &str, user: &str, image_path: &Path) -> Result<String> {
        self.generate(&self.vision_model, system, user, false, Some(image_path))
    }
}
