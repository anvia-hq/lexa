use crate::cli::Cli;
use anyhow::{Context, Result};
use lexa::engine;
use lexa::{freshness, mcp, snapshot};
use serde_json::json;
use std::io::IsTerminal;
use std::path::PathBuf;

use super::shared::*;

pub(crate) fn cmd_dump_tools() -> Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, &*lexa::mcp::tool_spec::TOOL_SPECS)?;
    use std::io::Write as _;
    writeln!(handle)?;
    Ok(())
}

pub(crate) fn cmd_index(root: &PathBuf, output: Option<&PathBuf>, cli: &Cli) -> Result<()> {
    let root = std::fs::canonicalize(root)?;
    let snap_path = if let Some(out) = output {
        out.clone()
    } else {
        graph_path_for_root(&root, cli)
    };

    if !cli.json {
        print_index_banner(&root, &snap_path);
    }

    let mut engine = engine::Engine::new(16384);
    let count = engine.index_project(&root);

    if cli.json {
        if !cli.no_graph {
            snapshot::write_snapshot(&engine, &snap_path)?;
        }
        return print_agent_result(json!({
            "root": root.display().to_string(),
            "files_indexed": count,
            "symbols_indexed": engine.symbol_index_count(),
            "unique_words_indexed": engine.word_index_count(),
            "word_indexed_files": engine.word_index_file_count(),
            "graph": (!cli.no_graph).then(|| snap_path.display().to_string()),
            "persisted": !cli.no_graph,
        }));
    }

    println!("Indexed {} files", count);
    println!("  Symbols: {}", engine.symbol_index_count());
    println!("  Unique words: {}", engine.word_index_count());
    println!("  Word-indexed files: {}", engine.word_index_file_count());

    if !cli.no_graph {
        snapshot::write_snapshot(&engine, &snap_path)?;
        println!("Graph saved to {}", snap_path.display());
    } else {
        println!("Graph not saved (--no-graph)");
    }

    Ok(())
}

pub(crate) fn print_index_banner(root: &std::path::Path, graph: &std::path::Path) {
    if std::io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").ok().as_deref() != Some("dumb")
    {
        println!(
            "\x1b[1;38;5;81mLexa\x1b[0m \x1b[38;5;245mFast code intelligence for AI agents\x1b[0m"
        );
        println!("\x1b[38;5;245mroot\x1b[0m  {}", root.display());
        println!("\x1b[38;5;245mgraph\x1b[0m {}", graph.display());
        println!();
    } else {
        println!("Indexing {}...", root.display());
    }
}

pub(crate) fn cmd_reindex(root: &PathBuf, cli: &Cli) -> Result<()> {
    cmd_index(root, None, cli)
}

pub(crate) fn cmd_clear_index(cli: &Cli) -> Result<()> {
    let snap_path = graph_path(cli)?;
    let existed = snap_path.exists();
    if existed {
        std::fs::remove_file(&snap_path)
            .with_context(|| format!("failed to remove graph {}", snap_path.display()))?;
    }

    if cli.json {
        return print_agent_result(json!({
            "graph": snap_path.display().to_string(),
            "removed": existed,
        }));
    }

    if existed {
        println!("Removed graph {}", snap_path.display());
    } else {
        println!("No graph file found at {}", snap_path.display());
    }
    Ok(())
}

pub(crate) fn cmd_mcp(
    path: &PathBuf,
    no_refresh: bool,
    debounce_ms: u64,
    log_file: Option<&PathBuf>,
    cli: &Cli,
) -> Result<()> {
    let mut diagnostics = match log_file {
        Some(path) => mcp::Diagnostics::append_to_path(path)?,
        None => mcp::Diagnostics::disabled(),
    };
    diagnostics.info(format!(
        "lexa {} starting MCP server",
        env!("CARGO_PKG_VERSION")
    ));
    diagnostics.info(format!("requested_root={}", path.display()));

    let root = match std::fs::canonicalize(path) {
        Ok(root) => root,
        Err(err) => {
            diagnostics.error(format!(
                "failed to resolve MCP root {}: {err}",
                path.display()
            ));
            return Err(err.into());
        }
    };
    let snap_path = graph_path_for_root(&root, cli);
    let mut engine = engine::Engine::new(16384);
    diagnostics.info(format!(
        "root={} graph={} persist_graph={} refresh={} watcher={} output=toon",
        root.display(),
        snap_path.display(),
        !cli.no_graph,
        !no_refresh,
        !no_refresh
    ));

    if !cli.no_graph && snap_path.exists() {
        match snapshot::load_snapshot_into_engine(&mut engine, &snap_path) {
            Ok(count) => mcp_info(
                &mut diagnostics,
                format!("Loaded {} files from graph", count),
            ),
            Err(err) => mcp_warn(&mut diagnostics, format!("Failed to load graph: {err}")),
        }
    }

    if engine.file_count() == 0 {
        mcp_info(
            &mut diagnostics,
            format!("Indexing {} for MCP...", root.display()),
        );
        let count = engine.index_project(&root);
        mcp_info(&mut diagnostics, format!("Indexed {} files", count));
        if !cli.no_graph {
            if let Err(err) = snapshot::write_snapshot(&engine, &snap_path) {
                diagnostics.error(format!(
                    "failed to save graph {}: {err}",
                    snap_path.display()
                ));
                return Err(err);
            }
            mcp_info(
                &mut diagnostics,
                format!("Graph saved to {}", snap_path.display()),
            );
        }
    } else if !no_refresh {
        mcp_info(&mut diagnostics, "Checking MCP graph freshness...");
        let summary = match freshness::refresh_project(&mut engine, &root) {
            Ok(summary) => summary,
            Err(err) => {
                diagnostics.error(format!(
                    "failed to refresh MCP graph for {}: {err}",
                    root.display()
                ));
                return Err(err);
            }
        };
        if summary.changed() {
            mcp_info(
                &mut diagnostics,
                format!(
                    "Refreshed MCP graph: {} indexed, {} removed",
                    summary.indexed, summary.removed
                ),
            );
            if !cli.no_graph {
                if let Err(err) = snapshot::write_snapshot(&engine, &snap_path) {
                    diagnostics.error(format!(
                        "failed to save graph {}: {err}",
                        snap_path.display()
                    ));
                    return Err(err);
                }
                mcp_info(
                    &mut diagnostics,
                    format!("Graph saved to {}", snap_path.display()),
                );
            }
        }
    }

    let mut server = mcp::McpServer::new(engine, root, snap_path, !cli.no_graph, diagnostics);
    if !no_refresh {
        server.enable_watcher(debounce_ms)?;
    }
    server.run()
}

pub(crate) fn mcp_info(diagnostics: &mut mcp::Diagnostics, message: impl AsRef<str>) {
    let message = message.as_ref();
    eprintln!("{message}");
    diagnostics.info(message);
}

pub(crate) fn mcp_warn(diagnostics: &mut mcp::Diagnostics, message: impl AsRef<str>) {
    let message = message.as_ref();
    eprintln!("Warning: {message}");
    diagnostics.warn(message);
}
