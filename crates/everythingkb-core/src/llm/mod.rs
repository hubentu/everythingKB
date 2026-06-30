use anyhow::Result;

use crate::config::{Config, LlmBackend};
use crate::llm::ollama::OllamaClient;
use crate::llm::openai::OpenAiClient;

pub use client::LlmClient;

pub mod client;
pub mod ollama;
pub mod openai;

pub fn build_client(config: &Config) -> Result<Box<dyn LlmClient>> {
    match config.llm.backend {
        LlmBackend::Openai => Ok(Box::new(OpenAiClient::new(config)?)),
        LlmBackend::Ollama => Ok(Box::new(OllamaClient::new(config)?)),
    }
}
