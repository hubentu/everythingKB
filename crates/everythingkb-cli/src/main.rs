use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use everythingkb_core::ingest::{init_kb, ingest_path, scan, watch_roots};
use everythingkb_core::kb::{self, KbPaths, WikiScope};
use everythingkb_core::query;
use everythingkb_core::visualize;

#[derive(Parser)]
#[command(name = "everythingkb", about = "Personal knowledge base in Rust")]
struct Cli {
    #[arg(long, global = true)]
    kb: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new knowledge base
    Init,
    /// Scan configured roots and ingest new/changed files
    Scan {
        #[arg(long)]
        dry_run: bool,
        /// List each file as it is processed
        #[arg(short, long)]
        verbose: bool,
    },
    /// Watch scan roots for changes
    Watch,
    /// Ingest a single file or directory
    Add {
        path: PathBuf,
        #[arg(long)]
        dry_run: bool,
        /// Re-convert and recompile even if the file is unchanged
        #[arg(long)]
        force: bool,
        /// List each file as it is processed
        #[arg(short, long)]
        verbose: bool,
    },
    /// Ask a question over the compiled wiki
    Query {
        question: String,
        /// Search the private wiki instead of public
        #[arg(long)]
        private: bool,
    },
    /// Interactive chat over the wiki
    Chat {
        question: Option<String>,
        #[arg(long, default_value = "default")]
        session: String,
        /// Chat over the private wiki instead of public
        #[arg(long)]
        private: bool,
    },
    /// Show registry and wiki stats
    Status,
    /// List indexed documents
    List,
    /// Render OKF knowledge graph as interactive HTML
    Visualize {
        /// Output HTML file (default: wiki/graph.html)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Open in default browser after generating
        #[arg(long)]
        open: bool,
        /// Include private wiki pages in the graph
        #[arg(long)]
        private: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => {
            let paths = init_kb(cli.kb)?;
            let config = paths.load_config()?;
            println!("Initialized KB at {}", paths.root.display());
            println!("Config: {}", paths.config_path.display());
            println!("LLM backend: {:?}", config.llm.backend);
            match config.llm.backend {
                everythingkb_core::config::LlmBackend::Openai => {
                    println!(
                        "OpenAI-compatible: {} model `{}`",
                        config.llm.openai_base_url.as_deref().unwrap_or("?"),
                        config
                            .llm
                            .openai_model
                            .as_deref()
                            .unwrap_or(&config.llm.ollama_model)
                    );
                }
                everythingkb_core::config::LlmBackend::Ollama => {
                    println!("Ollama model: `{}`", config.llm.ollama_model);
                    println!("Run: ollama pull {}", config.llm.ollama_model);
                }
            }
            print_scan_paths(&config);
        }
        Commands::Scan { dry_run, verbose } => {
            let kb = KbPaths::open(cli.kb)?;
            let stats = scan(&kb, dry_run, verbose)?;
            print_stats(&stats);
        }
        Commands::Watch => {
            let kb = KbPaths::open(cli.kb)?;
            watch_roots(&kb)?;
        }
        Commands::Add { path, dry_run, force, verbose } => {
            let kb = KbPaths::open(cli.kb)?;
            let stats = ingest_path(&kb, &path, dry_run, force, verbose)?;
            print_stats(&stats);
        }
        Commands::Query { question, private } => {
            let kb = KbPaths::open(cli.kb)?;
            let scope = if private {
                WikiScope::Private
            } else {
                WikiScope::Public
            };
            let answer = query::query(&kb, &question, scope)?;
            println!("{answer}");
        }
        Commands::Chat {
            question,
            session,
            private,
        } => {
            let kb = KbPaths::open(cli.kb)?;
            let scope = if private {
                WikiScope::Private
            } else {
                WikiScope::Public
            };
            if let Some(q) = question {
                let answer = query::chat_turn(&kb, &session, &q, scope)?;
                println!("{answer}");
            } else {
                query::chat_repl(&kb, &session, scope)?;
            }
        }
        Commands::Status => {
            let kb = KbPaths::open(cli.kb)?;
            let registry = kb.open_registry()?;
            let stats = registry.stats()?;
            let (pub_s, pub_c, pub_e) = kb::wiki_stats(&kb.wiki)?;
            let (priv_s, priv_c, priv_e) = kb::wiki_stats(&kb.private_wiki)?;
            println!("KB: {}", kb.root.display());
            println!("Registry: {} total, {} indexed, {} failed, {} pending",
                stats.total, stats.indexed, stats.failed, stats.pending);
            println!("Public wiki: {} summaries, {} concepts, {} entities",
                pub_s, pub_c, pub_e);
            println!("Private wiki: {} summaries, {} concepts, {} entities",
                priv_s, priv_c, priv_e);
            let config = kb.load_config()?;
            print_scan_paths(&config);
        }
        Commands::List => {
            let kb = KbPaths::open(cli.kb)?;
            let registry = kb.open_registry()?;
            for rec in registry.list_indexed()? {
                let tag = if rec.private { " [private]" } else { "" };
                println!(
                    "{}  [{}]{}",
                    rec.path,
                    rec.doc_name.unwrap_or_default(),
                    tag
                );
            }
        }
        Commands::Visualize { output, open, private } => {
            let kb = KbPaths::open(cli.kb)?;
            let out = output.unwrap_or_else(|| kb.wiki.join("graph.html"));
            let (nodes, edges) = visualize::write_visualization(&kb, &out, private)?;
            println!(
                "Wrote {} ({} nodes, {} edges)",
                out.display(),
                nodes,
                edges
            );
            if open {
                open_in_browser(&out)?;
            }
        }
    }
    Ok(())
}

fn print_scan_paths(config: &everythingkb_core::config::Config) {
    let paths = config
        .resolved_scan_paths()
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>();
    println!("Scan paths ({}): {}", paths.len(), paths.join(", "));
}

fn print_stats(stats: &everythingkb_core::ingest::IngestStats) {
    println!(
        "Ingest complete: {} added, {} skipped, {} failed",
        stats.added, stats.skipped, stats.failed
    );
}

fn open_in_browser(path: &PathBuf) -> Result<()> {
    use std::process::Command;
    let url = format!("file://{}", path.canonicalize()?.display());
    #[cfg(target_os = "linux")]
    Command::new("xdg-open").arg(&url).spawn()?;
    #[cfg(target_os = "macos")]
    Command::new("open").arg(&url).spawn()?;
    #[cfg(target_os = "windows")]
    Command::new("cmd").args(["/C", "start", "", &url]).spawn()?;
    Ok(())
}
