use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::classifier::{classify, FileKind};
use crate::compile;
use crate::config::Config;
use crate::convert;
use crate::kb::KbPaths;
use crate::metadata;
use crate::registry::{file_metadata, portable_path, FileStatus, Registry};
use crate::scanner;
use crate::wiki;

#[derive(Debug, Default)]
pub struct IngestStats {
    pub added: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn ingest_path(
    kb: &KbPaths,
    path: &Path,
    dry_run: bool,
    force: bool,
    verbose: bool,
) -> Result<IngestStats> {
    if path.is_dir() {
        let mut stats = IngestStats::default();
        for entry in walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let s = ingest_file(kb, entry.path(), dry_run, force, verbose)?;
            merge_stats(&mut stats, s);
        }
        return Ok(stats);
    }
    ingest_file(kb, path, dry_run, force, verbose)
}

pub fn scan(kb: &KbPaths, dry_run: bool, verbose: bool) -> Result<IngestStats> {
    let config = kb.load_config()?;
    let roots = config.resolved_scan_paths();
    eprintln!(
        "Scanning {} path(s): {}",
        roots.len(),
        roots
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut stats = IngestStats::default();
    scanner::for_each_hit(&config, |hit| {
        let s = ingest_file(kb, &hit.path, dry_run, false, verbose)?;
        merge_stats(&mut stats, s);
        Ok(())
    })?;
    Ok(stats)
}

fn log_ingest(verbose: bool, label: &str, path: &Path, detail: Option<&str>) {
    if !verbose {
        return;
    }
    if let Some(d) = detail {
        eprintln!("[{label}] {} ({d})", path.display());
    } else {
        eprintln!("[{label}] {}", path.display());
    }
}

fn merge_stats(total: &mut IngestStats, part: IngestStats) {
    total.added += part.added;
    total.skipped += part.skipped;
    total.failed += part.failed;
}

pub fn ingest_file(
    kb: &KbPaths,
    path: &Path,
    dry_run: bool,
    force: bool,
    verbose: bool,
) -> Result<IngestStats> {
    let config = kb.load_config()?;
    let registry = kb.open_registry()?;
    let key = portable_path(path, &kb.root);
    let (mtime, size) = file_metadata(path)?;
    let hash = Registry::hash_file(path)?;

    if !force && !registry.needs_reindex(path, mtime, size, &hash)? {
        log_ingest(verbose, "skip", path, Some("unchanged"));
        return Ok(IngestStats {
            skipped: 1,
            ..Default::default()
        });
    }

    let kind = classify(path, &config);
    if kind == FileKind::Skip {
        registry.upsert(
            &key,
            &hash,
            mtime,
            size,
            FileStatus::Skipped,
            None,
            Some("not indexable"),
        )?;
        log_ingest(verbose, "skip", path, Some("not indexable"));
        return Ok(IngestStats {
            skipped: 1,
            ..Default::default()
        });
    }

    if dry_run {
        log_ingest(verbose, "dry-run", path, None);
        return Ok(IngestStats {
            added: 1,
            ..Default::default()
        });
    }

    log_ingest(verbose, "ingest", path, None);

    match ingest_one(kb, path, kind, &registry, force) {
        Ok(doc_name) => {
            registry.upsert(
                &key,
                &hash,
                mtime,
                size,
                FileStatus::Indexed,
                Some(&doc_name),
                None,
            )?;
            wiki::append_log(
                &kb.wiki,
                "ingest",
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
            )?;
            log_ingest(verbose, "added", path, Some(&doc_name));
            Ok(IngestStats {
                added: 1,
                ..Default::default()
            })
        }
        Err(e) => {
            registry.upsert(
                &key,
                &hash,
                mtime,
                size,
                FileStatus::Failed,
                None,
                Some(&e.to_string()),
            )?;
            eprintln!("[ERROR] {}: {e:#}", path.display());
            log_ingest(verbose, "failed", path, None);
            Ok(IngestStats {
                failed: 1,
                ..Default::default()
            })
        }
    }
}

fn ingest_one(
    kb: &KbPaths,
    path: &Path,
    kind: FileKind,
    registry: &Registry,
    force: bool,
) -> Result<String> {
    let doc_name = convert::resolve_doc_name(path, kb, registry)?;

    let source = match kind {
        FileKind::Document => {
            let result = convert::convert_document(path, kb, registry, force)?;
            if result.skipped {
                return Ok(result.doc_name);
            }
            result
                .source_path
                .context("missing source after convert")?
        }
        FileKind::LongDocument => {
            let result = convert::convert_document(path, kb, registry, force)?;
            if result.skipped {
                return Ok(result.doc_name);
            }
            compile::compile_long_doc(kb, &doc_name, path, path)?;
            return Ok(doc_name);
        }
        FileKind::UserProfile => metadata::write_metadata_stub(path, kb, &doc_name)?,
        FileKind::Image => metadata::write_image_summary(path, kb, &doc_name)?,
        FileKind::Skip => anyhow::bail!("unexpected skip"),
    };

    compile::compile_short_doc(kb, &doc_name, &source, path)?;
    Ok(doc_name)
}

pub fn init_kb(root: Option<PathBuf>) -> Result<KbPaths> {
    KbPaths::init(root, &Config::default())
}

pub fn watch_roots(kb: &KbPaths) -> Result<()> {
    use notify_debouncer_full::{new_debouncer, DebounceEventResult};
    use notify::RecursiveMode;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let config = kb.load_config()?;
    let roots = config.resolved_scan_paths();
    let (tx, rx) = channel();
    let kb_root = kb.root.clone();

    let mut debouncer = new_debouncer(Duration::from_secs(5), None, move |res: DebounceEventResult| {
        let _ = tx.send(res);
    })?;

    for root in &roots {
        if root.exists() {
            debouncer.watch(root, RecursiveMode::Recursive)?;
        }
    }

    eprintln!("Watching {} roots (Ctrl+C to stop)...", roots.len());
    loop {
        if let Ok(Ok(events)) = rx.recv() {
            for event in events {
                for path in &event.paths {
                    if path.is_file() {
                        if let Ok(kb) = KbPaths::open(Some(kb_root.clone())) {
                            let _ = ingest_file(&kb, path, false, false, false);
                        }
                    }
                }
            }
        }
    }
}
