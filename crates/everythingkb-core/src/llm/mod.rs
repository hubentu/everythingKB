pub mod client;
pub mod ollama;

use anyhow::Result;

use crate::config::Config;

pub use client::LlmClient;

pub fn build_client(config: &Config) -> Result<Box<dyn LlmClient>> {
    Ok(Box::new(ollama::OllamaClient::new(config)?))
}
