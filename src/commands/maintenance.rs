use crate::cli::Cli;
use anyhow::{bail, Result};
use lexa::application::ProjectSession;
use lexa::engine;
use lexa::output::format_unix_ms_utc;
use lexa::{audit, pipeline, snapshot};
use serde_json::json;
use std::path::{Path, PathBuf};

use super::shared::*;

use crate::cli::AuditInclude;

pub(crate) fn cmd_glob(pattern: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let results = engine.glob_files(pattern);

    if cli.json {
        return print_agent_result(json!({
            "pattern": pattern,
            "count": results.len(),
            "paths": results,
        }));
    }

    if results.is_empty() {
        println!("No files match '{}'", pattern);
    } else {
        println!("{} files match '{}':", results.len(), pattern);
        for path in &results {
            println!("  {}", path);
        }
    }

    Ok(())
}

pub(crate) fn cmd_ls(path: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let entries = engine.list_dir(path);

    if cli.json {
        return print_agent_result(json!({
            "path": path,
            "count": entries.len(),
            "entries": entries.into_iter().map(|(name, meta)| {
                if let Some(meta) = meta {
                    json!({
                        "name": name,
                        "kind": "file",
                        "language": meta.language.as_str(),
                        "line_count": meta.line_count,
                        "byte_size": meta.byte_size,
                        "symbol_count": meta.symbol_count,
                        "modified_ms": meta.modified_ms,
                        "modified_utc": format_unix_ms_utc(meta.modified_ms),
                    })
                } else {
                    json!({"name": name, "kind": "directory"})
                }
            }).collect::<Vec<_>>()
        }));
    }

    if entries.is_empty() {
        println!("No files in '{}'", path);
    } else {
        for (name, meta) in &entries {
            if let Some(m) = meta {
                println!(
                    "{:<60} {:>8} {:>6}L {:>4} sym",
                    name,
                    m.language.as_str(),
                    m.line_count,
                    m.symbol_count
                );
            } else {
                println!("{}/", name);
            }
        }
    }

    Ok(())
}

pub(crate) fn cmd_status(cli: &Cli) -> Result<()> {
    let loaded = load_engine_for_root(&current_root()?, cli)?;
    let engine = loaded.engine;
    let snap_path = graph_path(cli)?;
    let graph = if snap_path.exists() {
        let metadata = std::fs::metadata(&snap_path)?;
        json!({
            "path": snap_path.display().to_string(),
            "exists": true,
            "size_bytes": metadata.len(),
            "size_mb": metadata.len() as f64 / (1024.0 * 1024.0),
        })
    } else {
        json!({"path": snap_path.display().to_string(), "exists": false})
    };

    if cli.json {
        return print_agent_result(json!({
            "files_indexed": engine.file_count(),
            "symbols_indexed": engine.symbol_index_count(),
            "unique_words_indexed": engine.word_index_count(),
            "word_indexed_files": engine.word_index_file_count(),
            "seq": engine.store().current_seq(),
            "change_history_persisted": false,
            "graph": graph,
            "refresh": {
                "indexed": loaded.refresh.indexed,
                "removed": loaded.refresh.removed,
                "changed": loaded.refresh.changed(),
            },
        }));
    }

    println!("lexa status:");
    println!("  Files indexed: {}", engine.file_count());
    println!("  Symbols indexed: {}", engine.symbol_index_count());
    println!("  Unique words indexed: {}", engine.word_index_count());
    println!("  Word-indexed files: {}", engine.word_index_file_count());
    println!(
        "  Current sequence: {} (session-local)",
        engine.store().current_seq()
    );
    println!("  Change history persisted: false");

    if snap_path.exists() {
        let metadata = std::fs::metadata(&snap_path)?;
        println!(
            "  Graph: {} ({:.1} MB)",
            snap_path.display(),
            metadata.len() as f64 / (1024.0 * 1024.0)
        );
    } else {
        println!("  Graph: not found");
    }

    Ok(())
}

pub(crate) fn cmd_audit(
    max: Option<usize>,
    since: Option<&str>,
    strict: bool,
    config_path: Option<&PathBuf>,
    no_config: bool,
    include: &[AuditInclude],
    cli: &Cli,
) -> Result<()> {
    let mut engine = load_engine(cli)?;
    if engine.file_count() == 0 {
        bail!("no files indexed; run 'lexa index .' before running audit");
    }
    let root = std::env::current_dir()?;
    let config = audit::load_audit_config(&root, config_path.map(PathBuf::as_path), no_config)?;
    let scope = if let Some(base) = since {
        audit::AuditScope::GitSince {
            base: base.to_string(),
            changed_files: audit::changed_files_since(&root, base)?,
        }
    } else {
        audit::AuditScope::Project
    };
    let report =
        ProjectSession::new(&mut engine, &root, Path::new(""), false).audit(audit::AuditOptions {
            max_results: max,
            scope,
            config,
            includes: audit_includes(include),
        });

    if cli.json {
        print_agent_result(json!(report))?;
    } else {
        print!("{}", audit::render_audit_report(&report));
    }

    if strict && report.summary.high > 0 {
        std::process::exit(1);
    }

    Ok(())
}

pub(crate) fn audit_includes(values: &[AuditInclude]) -> audit::AuditIncludes {
    audit::AuditIncludes {
        dead_code: values.contains(&AuditInclude::DeadCode),
    }
}

pub(crate) fn cmd_watch(path: &str, debounce_ms: u64, cli: &Cli) -> Result<()> {
    use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let watch_path = std::fs::canonicalize(path)?;
    println!("Watching {} for changes...", watch_path.display());
    println!("Press Ctrl+C to stop");

    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        tx,
        notify::Config::default().with_poll_interval(Duration::from_millis(debounce_ms)),
    )?;

    watcher.watch(&watch_path, RecursiveMode::Recursive)?;

    let mut engine = engine::Engine::new(16384);
    let snap_path = graph_path_for_root(&watch_path, cli);
    if !cli.no_graph {
        if snap_path.exists() {
            match snapshot::load_snapshot_into_engine(&mut engine, &snap_path) {
                Ok(count) => eprintln!("Loaded {} files from graph", count),
                Err(err) => bail!(
                    "failed to load graph {}: {err}. Run 'lexa reindex {}' to rebuild it or 'lexa clear-index' to remove it.",
                    snap_path.display(),
                    watch_path.display()
                ),
            }
        } else {
            bail!(
                "no graph file found at {}. Run 'lexa index {}' first.",
                snap_path.display(),
                watch_path.display()
            );
        }
    }

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                let Event { kind, paths, .. } = event;
                let should_reindex = matches!(
                    kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                );

                if should_reindex {
                    for path in &paths {
                        if let Ok(relative) = path.strip_prefix(&watch_path) {
                            let relative_str = relative.to_string_lossy().to_string();

                            match kind {
                                EventKind::Create(_) | EventKind::Modify(_) => {
                                    if let Ok(content) = std::fs::read_to_string(path) {
                                        engine.index_file(&relative_str, &content);
                                        println!("Updated: {}", relative_str);
                                    }
                                }
                                EventKind::Remove(_) => {
                                    engine.remove_file(&relative_str);
                                    println!("Removed: {}", relative_str);
                                }
                                _ => {}
                            }
                        }
                    }

                    if !cli.no_graph {
                        if let Err(e) = snapshot::write_snapshot(&engine, &snap_path) {
                            eprintln!("Warning: Failed to save graph: {}", e);
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {}", e);
            }
            Err(e) => {
                eprintln!("Channel error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

pub(crate) fn cmd_pipeline(pipeline: &[String], cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let pipeline_str = pipeline.join(" ");
    let output = pipeline::run_output(&engine, &pipeline_str);
    let text = output.render();
    if cli.json {
        return print_agent_result(output.to_json(&pipeline_str));
    }
    println!("{}", text);
    Ok(())
}
