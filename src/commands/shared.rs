use crate::cli::Cli;
use anyhow::{bail, Context, Result};
use lexa::engine;
use lexa::output::agent_toon;
use lexa::{edit, freshness, snapshot};
use std::path::{Path, PathBuf};

const DEFAULT_GRAPH_PATH: &str = ".lexa/graph.lexa";

pub(crate) fn reject_removed_output_flags() {
    if let Some(flag) = removed_output_flag(std::env::args().skip(1)) {
        eprintln!("{flag} was removed; Lexa command results are always TOON structured text.");
        std::process::exit(2);
    }
}

pub(crate) fn removed_output_flag(
    args: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<&'static str> {
    for arg in args {
        let arg = arg.as_ref();
        if arg == "--" {
            break;
        }
        match arg {
            "--json" => return Some("--json"),
            "--structured-content" => return Some("--structured-content"),
            "--json-output" => return Some("--json-output"),
            _ if arg.starts_with("--json=") => return Some("--json"),
            _ if arg.starts_with("--structured-content=") => return Some("--structured-content"),
            _ if arg.starts_with("--json-output=") => return Some("--json-output"),
            _ => {}
        }
    }
    None
}

pub(crate) fn current_root() -> Result<PathBuf> {
    std::fs::canonicalize(std::env::current_dir()?)
        .context("failed to canonicalize current directory")
}

pub(crate) fn graph_path_for_root(root: &std::path::Path, cli: &Cli) -> PathBuf {
    cli.graph
        .clone()
        .unwrap_or_else(|| root.join(DEFAULT_GRAPH_PATH))
}

pub(crate) fn graph_path(cli: &Cli) -> Result<PathBuf> {
    let root = current_root()?;
    Ok(graph_path_for_root(&root, cli))
}

pub(crate) struct LoadedEngine {
    pub(crate) engine: engine::Engine,
    pub(crate) refresh: freshness::RefreshSummary,
}

pub(crate) fn load_engine(cli: &Cli) -> Result<engine::Engine> {
    let root = current_root()?;
    Ok(load_engine_for_root(&root, cli)?.engine)
}

pub(crate) fn load_engine_for_root(root: &Path, cli: &Cli) -> Result<LoadedEngine> {
    let mut engine = engine::Engine::new(16384);
    let path = graph_path_for_root(root, cli);
    let mut loaded_graph = false;
    let mut refresh = freshness::RefreshSummary::default();

    if !cli.no_graph {
        if path.exists() {
            match snapshot::load_snapshot_into_engine(&mut engine, &path) {
                Ok(count) => {
                    eprintln!("Loaded {} files from graph", count);
                    loaded_graph = true;
                }
                Err(e) => {
                    bail!(
                        "failed to load graph {}: {e}. Run 'lexa reindex .' to rebuild it or 'lexa clear-index' to remove it.",
                        path.display()
                    );
                }
            }
        } else {
            eprintln!(
                "No graph file found at {}. Run 'lexa index .' first.",
                path.display()
            );
        }
    }

    if loaded_graph {
        refresh = refresh_loaded_graph(&mut engine, root, &path, !cli.no_graph)?;
    }

    Ok(LoadedEngine { engine, refresh })
}

pub(crate) fn load_existing_engine_for_root(
    root: &Path,
    cli: &Cli,
) -> Result<(engine::Engine, PathBuf)> {
    let snap_path = graph_path_for_root(root, cli);
    if !snap_path.exists() {
        bail!(
            "no graph file found at {}. Run 'lexa index .' first.",
            snap_path.display()
        );
    }

    let mut engine = engine::Engine::new(16384);
    snapshot::load_snapshot_into_engine(&mut engine, &snap_path).with_context(|| {
        format!(
            "failed to load graph {}. Run 'lexa reindex .' to rebuild it or 'lexa clear-index' to remove it.",
            snap_path.display()
        )
    })?;
    refresh_loaded_graph(&mut engine, root, &snap_path, true)?;
    Ok((engine, snap_path))
}

pub(crate) fn refresh_loaded_graph(
    engine: &mut engine::Engine,
    root: &Path,
    snap_path: &Path,
    persist_graph: bool,
) -> Result<freshness::RefreshSummary> {
    eprintln!("Checking graph freshness...");
    let refresh = freshness::refresh_project(engine, root)
        .with_context(|| format!("failed to refresh graph for {}", root.display()))?;
    if refresh.changed() {
        eprintln!(
            "Refreshed graph: {} indexed, {} removed",
            refresh.indexed, refresh.removed
        );
        if persist_graph {
            snapshot::write_snapshot(engine, snap_path)?;
        }
    }
    Ok(refresh)
}

pub(crate) fn required_text(
    positional: Option<&str>,
    flag: Option<&str>,
    command: &str,
    label: &str,
) -> Result<String> {
    match (positional, flag) {
        (Some(_), Some(_)) => {
            bail!("{command} accepts either positional {label} or --query, not both")
        }
        (Some(value), None) | (None, Some(value)) => Ok(value.to_string()),
        (None, None) => bail!("{command} requires {label}. Example: lexa {command} <{label}>"),
    }
}

pub(crate) fn max_limit(
    max: Option<usize>,
    max_results: Option<usize>,
    default: usize,
) -> Result<usize> {
    match (max, max_results) {
        (Some(_), Some(_)) => bail!("use either --max or --max-results, not both"),
        (Some(value), None) | (None, Some(value)) => Ok(value),
        (None, None) => Ok(default),
    }
}

pub(crate) fn resolve_line_range(
    line_range: Option<&str>,
    line_start: Option<u32>,
    line_end: Option<u32>,
) -> Result<(Option<u32>, Option<u32>)> {
    if line_range.is_some() && (line_start.is_some() || line_end.is_some()) {
        bail!("use either --line-range or --line-start/--line-end, not both");
    }
    if let Some(range) = line_range {
        return parse_line_range(range);
    }
    Ok((line_start, line_end))
}

pub(crate) fn parse_line_range(range: &str) -> Result<(Option<u32>, Option<u32>)> {
    if let Some((start, end)) = range.split_once('-') {
        let start = if start.is_empty() {
            None
        } else {
            Some(start.parse::<u32>()?)
        };
        let end = if end.is_empty() {
            None
        } else {
            Some(end.parse::<u32>()?)
        };
        Ok((start, end))
    } else {
        let line = range.parse::<u32>()?;
        Ok((Some(line), Some(line)))
    }
}

pub(crate) fn print_agent_result(value: serde_json::Value) -> Result<()> {
    println!("{}", agent_toon(&current_command_tool(), value)?);
    Ok(())
}

pub(crate) fn current_command_tool() -> String {
    let command_names = [
        ("index", "index"),
        ("reindex", "reindex"),
        ("clear-index", "clear_index"),
        ("files", "files"),
        ("list", "list"),
        ("path-search", "path_search"),
        ("text-search", "text_search"),
        ("outline", "outline"),
        ("symbol-defs", "symbol_defs"),
        ("symbol-search", "symbol_search"),
        ("word-refs", "word_refs"),
        ("trace-deps", "trace_deps"),
        ("recent", "recent"),
        ("callers", "callers"),
        ("brief", "brief"),
        ("changes", "changes"),
        ("read", "read"),
        ("patch", "patch"),
        ("create", "create"),
        ("glob", "glob"),
        ("status", "status"),
        ("audit", "audit"),
        ("pipeline", "pipeline"),
        ("mcp", "mcp"),
        ("upgrade", "upgrade"),
        ("update", "upgrade"),
    ];

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--graph" {
            let _ = args.next();
            continue;
        }
        if arg == "--no-graph"
            || arg == "--version"
            || arg.starts_with("--graph=")
            || arg.starts_with('-')
        {
            continue;
        }
        if let Some((_, tool)) = command_names.iter().find(|(command, _)| *command == arg) {
            return (*tool).to_string();
        }
    }

    "result".to_string()
}

pub(crate) fn edit_op_str(op: edit::EditOp) -> &'static str {
    match op {
        edit::EditOp::Replace => "replace",
        edit::EditOp::Insert => "insert",
        edit::EditOp::Delete => "delete",
    }
}

pub(crate) fn preview_mode_str(mode: edit::PreviewMode) -> &'static str {
    match mode {
        edit::PreviewMode::Compact => "compact",
        edit::PreviewMode::Full => "full",
    }
}
