//! everythingKB core library — personal knowledge base in Rust.

pub mod classifier;
pub mod compile;
pub mod config;
pub mod convert;
pub mod exclusions;
pub mod index;
pub mod ingest;
pub mod kb;
pub mod llm;
pub mod metadata;
pub mod privacy;
pub mod query;
pub mod registry;
pub mod scanner;
pub mod sessions;
pub mod visualize;
pub mod wiki;

pub use config::Config;
pub use kb::{KbPaths, WikiLayout, WikiScope};
