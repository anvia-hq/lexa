mod cache;
mod edit;
mod engine;
mod glob;
mod index;
mod mcp;
mod parser;
mod pipeline;
mod snapshot;
mod store;
mod types;
mod walker;

use anyhow::Result;
use clap::{Parser, Subcommand};
use engine::SearchOptions;
use serde_json::json;
use std::path::PathBuf;

const DEFAULT_GRAPH_PATH: &str = ".lexa/graph.lexa";

#[derive(Parser)]
#[command(
    name = "lexa",
    version,
    about = "Fast code intelligence engine for AI agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true)]
    graph: Option<PathBuf>,

    #[arg(long = "no-graph", global = true)]
    no_graph: bool,

    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    Index {
        path: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Map {
        #[arg(default_value = "")]
        path: String,
    },

    List {
        #[arg(default_value = "")]
        path: String,
    },

    #[command(name = "find-path")]
    FindPath {
        pattern: String,

        #[arg(short, long, default_value = "20")]
        max: usize,
    },

    Search {
        query: String,

        #[arg(short, long, default_value = "20")]
        max: usize,

        #[arg(short, long)]
        regex: bool,

        #[arg(long)]
        scope: bool,

        #[arg(short, long)]
        compact: bool,

        #[arg(long)]
        paths_only: bool,

        #[arg(long)]
        path_glob: Option<String>,
    },

    Outline {
        path: String,
    },

    #[command(name = "find-symbol")]
    FindSymbol {
        name: String,
    },

    #[command(name = "find-word")]
    FindWord {
        word: String,
    },

    #[command(name = "trace-deps")]
    Deps {
        path: String,

        #[arg(short, long)]
        reverse: bool,

        #[arg(short, long)]
        transitive: bool,
    },

    Recent {
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    #[command(name = "find-callers")]
    FindCallers {
        name: String,

        #[arg(short, long, default_value = "20")]
        max: usize,
    },

    Brief {
        task: String,

        #[arg(short, long, default_value = "10")]
        max: usize,
    },

    Changes {
        #[arg(default_value = "0")]
        since: u64,
    },

    Read {
        path: String,

        #[arg(short = 'L', long)]
        line_range: Option<String>,

        #[arg(short, long)]
        compact: bool,

        #[arg(long)]
        if_hash: Option<String>,

        #[arg(long)]
        hash: bool,
    },

    Patch {
        path: String,

        #[arg(value_enum)]
        op: edit::EditOp,

        #[arg(short = 'L', long)]
        line_range: Option<String>,

        #[arg(long)]
        after: Option<u32>,

        #[arg(long)]
        content: Option<String>,

        #[arg(long)]
        content_file: Option<PathBuf>,

        #[arg(long)]
        if_hash: Option<String>,

        #[arg(long)]
        dry_run: bool,
    },

    Glob {
        pattern: String,
    },

    Status,

    Watch {
        #[arg(default_value = ".")]
        path: String,

        #[arg(short, long, default_value = "500")]
        debounce: u64,
    },

    Pipeline {
        #[arg(trailing_var_arg = true)]
        pipeline: Vec<String>,
    },

    Mcp {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index {
            ref path,
            ref output,
        } => cmd_index(path, output.as_ref(), &cli),
        Commands::Map { ref path } => cmd_tree(path, &cli),
        Commands::List { ref path } => cmd_ls(path, &cli),
        Commands::FindPath { ref pattern, max } => cmd_find(pattern, max, &cli),
        Commands::Search {
            ref query,
            max,
            regex,
            scope,
            compact,
            paths_only,
            ref path_glob,
        } => cmd_search(
            query,
            SearchOptions {
                max_results: max,
                regex,
                scope,
                compact,
                paths_only,
                path_glob: path_glob.clone(),
            },
            &cli,
        ),
        Commands::Outline { ref path } => cmd_outline(path, &cli),
        Commands::FindSymbol { ref name } => cmd_symbol(name, &cli),
        Commands::FindWord { ref word } => cmd_word(word, &cli),
        Commands::Deps {
            ref path,
            reverse,
            transitive,
        } => cmd_deps(path, reverse, transitive, &cli),
        Commands::Recent { limit } => cmd_hot(limit, &cli),
        Commands::FindCallers { ref name, max } => cmd_callers(name, max, &cli),
        Commands::Brief { ref task, max } => cmd_context(task, max, &cli),
        Commands::Changes { since } => cmd_changes(since, &cli),
        Commands::Read {
            ref path,
            ref line_range,
            compact,
            ref if_hash,
            hash,
        } => cmd_read(
            path,
            line_range.as_deref(),
            compact,
            if_hash.as_deref(),
            hash,
            &cli,
        ),
        Commands::Patch {
            ref path,
            op,
            ref line_range,
            after,
            ref content,
            ref content_file,
            ref if_hash,
            dry_run,
        } => cmd_edit(
            path,
            op,
            line_range.as_deref(),
            after,
            content.as_deref(),
            content_file.as_ref(),
            if_hash.as_deref(),
            dry_run,
            &cli,
        ),
        Commands::Glob { ref pattern } => cmd_glob(pattern, &cli),
        Commands::Status => cmd_status(&cli),
        Commands::Watch { ref path, debounce } => cmd_watch(path, debounce, &cli),
        Commands::Pipeline { ref pipeline } => cmd_query(pipeline, &cli),
        Commands::Mcp { ref path } => cmd_mcp(path, &cli),
    }
}

fn graph_path(cli: &Cli) -> PathBuf {
    cli.graph
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GRAPH_PATH))
}

fn project_graph_path(root: &std::path::Path, cli: &Cli) -> PathBuf {
    cli.graph
        .clone()
        .unwrap_or_else(|| root.join(DEFAULT_GRAPH_PATH))
}

fn load_engine(cli: &Cli) -> engine::Engine {
    let mut engine = engine::Engine::new(16384);

    if !cli.no_graph {
        let path = graph_path(cli);
        if path.exists() {
            match snapshot::load_snapshot_into_engine(&mut engine, &path) {
                Ok(count) => {
                    eprintln!("Loaded {} files from graph", count);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load graph: {}", e);
                }
            }
        } else {
            eprintln!(
                "No graph file found at {}. Run 'lexa index .' first.",
                path.display()
            );
        }
    }

    engine
}

fn cmd_index(root: &PathBuf, output: Option<&PathBuf>, cli: &Cli) -> Result<()> {
    let root = std::fs::canonicalize(root)?;

    if !cli.json {
        println!("Indexing {}...", root.display());
    }

    let mut engine = engine::Engine::new(16384);
    let count = engine.index_project(&root);
    let snap_path = if let Some(out) = output {
        out.clone()
    } else {
        graph_path(cli)
    };

    if cli.json {
        snapshot::write_snapshot(&engine, &snap_path)?;
        return print_json(json!({
            "root": root.display().to_string(),
            "files_indexed": count,
            "symbols_indexed": engine.symbol_index_count(),
            "unique_words_indexed": engine.word_index_count(),
            "word_indexed_files": engine.word_index_file_count(),
            "graph": snap_path.display().to_string(),
        }));
    }

    println!("Indexed {} files", count);
    println!("  Symbols: {}", engine.symbol_index_count());
    println!("  Unique words: {}", engine.word_index_count());
    println!("  Word-indexed files: {}", engine.word_index_file_count());

    snapshot::write_snapshot(&engine, &snap_path)?;
    println!("Graph saved to {}", snap_path.display());

    Ok(())
}

fn cmd_mcp(path: &PathBuf, cli: &Cli) -> Result<()> {
    let root = std::fs::canonicalize(path)?;
    let snap_path = project_graph_path(&root, cli);
    let mut engine = engine::Engine::new(16384);

    if !cli.no_graph && snap_path.exists() {
        match snapshot::load_snapshot_into_engine(&mut engine, &snap_path) {
            Ok(count) => eprintln!("Loaded {} files from graph", count),
            Err(err) => eprintln!("Warning: Failed to load graph: {err}"),
        }
    }

    if engine.file_count() == 0 {
        eprintln!("Indexing {} for MCP...", root.display());
        let count = engine.index_project(&root);
        eprintln!("Indexed {} files", count);
        if !cli.no_graph {
            snapshot::write_snapshot(&engine, &snap_path)?;
            eprintln!("Graph saved to {}", snap_path.display());
        }
    }

    let mut server = mcp::McpServer::new(engine, root, snap_path, !cli.no_graph);
    server.run()
}

fn cmd_search(query: &str, options: SearchOptions, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);

    let results = match engine.search_rich(query, &options) {
        Ok(results) => results,
        Err(e) => {
            eprintln!("Error: {}", e);
            return Ok(());
        }
    };

    if cli.json {
        return print_json(json!({
            "query": query,
            "count": results.len(),
            "limit": options.max_results,
            "regex": options.regex,
            "scope": options.scope,
            "compact": options.compact,
            "paths_only": options.paths_only,
            "path_glob": options.path_glob,
            "results": rich_results_json(&results),
        }));
    }

    if results.is_empty() {
        println!("No results found for '{}'", query);
        return Ok(());
    }

    println!("{} results for '{}':", results.len(), query);
    for result in &results {
        if options.paths_only {
            println!("  {}:{}", result.path, result.line_num);
        } else if let Some(scope) = &result.scope {
            println!(
                "  {}:{}: {}  [{} {}:{}-{}]",
                result.path,
                result.line_num,
                result.line_text,
                scope.kind,
                scope.name,
                scope.line_start,
                scope.line_end
            );
        } else {
            println!(
                "  {}:{}: {}",
                result.path, result.line_num, result.line_text
            );
        }
    }

    Ok(())
}

fn cmd_outline(path: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);

    match engine.get_outline(path) {
        Some(outline) => {
            if cli.json {
                return print_json(json!({
                    "path": path,
                    "language": outline.language.as_str(),
                    "line_count": outline.line_count,
                    "byte_size": outline.byte_size,
                    "symbol_count": outline.symbols.len(),
                    "imports": &outline.imports,
                    "symbols": &outline.symbols,
                }));
            }
            println!(
                "{} ({} lines, {} symbols)",
                path,
                outline.line_count,
                outline.symbols.len()
            );
            println!("Language: {}", outline.language);
            println!();

            if !outline.imports.is_empty() {
                println!("Imports:");
                for import in &outline.imports {
                    println!("  {}", import);
                }
                println!();
            }

            if !outline.symbols.is_empty() {
                println!("Symbols:");
                for sym in &outline.symbols {
                    let detail = sym.detail.as_deref().unwrap_or("");
                    let detail_str = if detail.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", detail)
                    };
                    println!(
                        "  L{:<5} {:<12} {}{}",
                        sym.line_start, sym.kind, sym.name, detail_str
                    );
                }
            }
        }
        None => {
            if cli.json {
                return print_json(json!({
                    "error": "file_not_found",
                    "path": path,
                    "available": engine.list_dir("").into_iter().take(20).map(|(name, _)| name).collect::<Vec<_>>(),
                }));
            }
            println!("File not found: {}", path);
            println!("Available files:");
            for file in engine.list_dir("").iter().take(20) {
                println!("  {}", file.0);
            }
        }
    }

    Ok(())
}

fn cmd_symbol(name: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let results = engine.find_symbol(name);

    if cli.json {
        return print_json(json!({"name": name, "count": results.len(), "results": results}));
    }

    if results.is_empty() {
        println!("No symbols found for '{}'", name);
        return Ok(());
    }

    println!("{} definition(s) for '{}':", results.len(), name);
    for result in &results {
        let detail = result.symbol.detail.as_deref().unwrap_or("");
        let detail_str = if detail.is_empty() {
            String::new()
        } else {
            format!(" {}", detail)
        };
        println!(
            "  {}:{}-{} {} {}{}",
            result.path,
            result.symbol.line_start,
            result.symbol.line_end,
            result.symbol.kind,
            result.symbol.name,
            detail_str
        );
    }

    Ok(())
}

fn cmd_word(word: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let results = engine.search_word(word);

    if cli.json {
        return print_json(json!({"word": word, "count": results.len(), "results": results}));
    }

    if results.is_empty() {
        println!("No occurrences of '{}'", word);
        return Ok(());
    }

    println!("{} occurrence(s) of '{}':", results.len(), word);
    for result in &results {
        println!(
            "  {}:{}: {}",
            result.path, result.line_num, result.line_text
        );
    }

    Ok(())
}

fn cmd_find(pattern: &str, max: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let results = engine.fuzzy_find(pattern, max);

    if cli.json {
        return print_json(json!({
            "query": pattern,
            "count": results.len(),
            "limit": max,
            "results": results.into_iter().map(|(path, score)| json!({
                "path": path,
                "score": score,
            })).collect::<Vec<_>>()
        }));
    }

    if results.is_empty() {
        println!("No files found matching '{}'", pattern);
        return Ok(());
    }

    println!("{} file(s) matching '{}':", results.len(), pattern);
    for (path, score) in &results {
        println!("  {} (score: {:.1})", path, score);
    }

    Ok(())
}

fn cmd_tree(path: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);

    if cli.json {
        if path.is_empty() {
            return print_json(json!({
                "path": path,
                "count": engine.file_count(),
                "files": engine.file_map().into_iter().map(|(path, meta)| json!({
                    "path": path,
                    "language": meta.language.as_str(),
                    "line_count": meta.line_count,
                    "byte_size": meta.byte_size,
                    "symbol_count": meta.symbol_count,
                    "modified_ms": meta.modified_ms,
                    "modified_utc": format_unix_ms_utc(meta.modified_ms),
                })).collect::<Vec<_>>()
            }));
        }

        let entries = engine.list_dir(path);
        return print_json(json!({
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

    if path.is_empty() {
        print!("{}", engine.get_tree());
    } else {
        let entries = engine.list_dir(path);
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
                println!("{:<60} [dir]", name);
            }
        }
    }

    Ok(())
}

fn cmd_deps(path: &str, reverse: bool, transitive: bool, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);

    let deps = if transitive {
        if reverse {
            engine.get_transitive_imported_by(path)
        } else {
            engine.get_transitive_depends_on(path)
        }
    } else if reverse {
        engine.get_imported_by(path)
    } else {
        engine.get_depends_on(path)
    };

    let label = if reverse { "imported by" } else { "depends on" };
    let transitive_label = if transitive { " (transitive)" } else { "" };

    if cli.json {
        return print_json(json!({
            "path": path,
            "direction": if reverse { "imported_by" } else { "depends_on" },
            "transitive": transitive,
            "count": deps.len(),
            "dependencies": deps,
        }));
    }

    if deps.is_empty() {
        println!("No {} dependencies for {}{}", label, path, transitive_label);
    } else {
        println!("{} {}{}: ", deps.len(), label, transitive_label);
        for dep in &deps {
            println!("  {}", dep);
        }
    }

    Ok(())
}

fn cmd_hot(limit: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let files = engine.get_hot_files(limit);

    if cli.json {
        return print_json(json!({
            "count": files.len(),
            "limit": limit,
            "files": files.into_iter().map(|(path, meta)| json!({
                "path": path,
                "language": meta.language.as_str(),
                "line_count": meta.line_count,
                "byte_size": meta.byte_size,
                "symbol_count": meta.symbol_count,
                "modified_ms": meta.modified_ms,
                "modified_utc": format_unix_ms_utc(meta.modified_ms),
            })).collect::<Vec<_>>()
        }));
    }

    if files.is_empty() {
        println!("No files indexed");
        return Ok(());
    }

    println!("{} recently modified file(s):", files.len().min(limit));
    for (path, meta) in &files {
        println!(
            "  {}  {:>6}L  {}",
            format_unix_ms_utc(meta.modified_ms),
            meta.line_count,
            path
        );
    }

    Ok(())
}

fn cmd_callers(name: &str, max: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let results = engine.find_callers(name, max);

    if cli.json {
        return print_json(json!({
            "name": name,
            "count": results.len(),
            "limit": max,
            "results": results,
        }));
    }

    if results.is_empty() {
        println!("No callers found for '{}'", name);
        return Ok(());
    }

    println!("{} caller(s) of '{}':", results.len(), name);
    for result in &results {
        println!(
            "  {}:{}: {}",
            result.path, result.line_num, result.line_text
        );
    }

    Ok(())
}

fn cmd_context(task: &str, max: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    if cli.json {
        return print_json(json!(engine.build_context_details(task, max)));
    }
    let context = engine.build_context(task, max);
    println!("{}", context);
    Ok(())
}

fn cmd_changes(since: u64, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let changes = engine.get_changes(since);

    if cli.json {
        return print_json(json!({
            "since": since,
            "count": changes.len(),
            "change_history_persisted": false,
            "note": "Change history is session-local and is not restored from graph snapshots.",
            "changes": changes.into_iter().map(|(path, seq, op)| json!({
                "path": path,
                "seq": seq,
                "op": op,
            })).collect::<Vec<_>>()
        }));
    }

    if changes.is_empty() {
        println!("No changes since sequence {} in this session", since);
        println!("Note: change history is session-local and is not restored from graph snapshots.");
        return Ok(());
    }

    println!("{} change(s) since sequence {}:", changes.len(), since);
    for (path, seq, op) in &changes {
        println!("  {} (seq {}): {}", path, seq, op);
    }

    Ok(())
}

fn cmd_read(
    path: &str,
    line_range: Option<&str>,
    compact: bool,
    if_hash: Option<&str>,
    show_hash: bool,
    cli: &Cli,
) -> Result<()> {
    let engine = load_engine(cli);

    let (line_start, line_end) = if let Some(range) = line_range {
        parse_line_range(range)?
    } else {
        (None, None)
    };

    match engine.read_file_rich(path, line_start, line_end, compact, if_hash) {
        Some(result) => {
            if cli.json {
                return print_json(json!({
                    "path": path,
                    "hash": format!("{:x}", result.hash),
                    "unchanged": result.unchanged,
                    "line_start": line_start,
                    "line_end": line_end,
                    "compact": compact,
                    "content": result.content,
                }));
            }
            if result.unchanged {
                println!("unchanged:{:x}", result.hash);
                return Ok(());
            }
            if show_hash || if_hash.is_some() {
                println!("hash:{:x}", result.hash);
            }
            print!("{}", result.content);
        }
        None => {
            if cli.json {
                return print_json(json!({"error": "file_not_found", "path": path}));
            }
            println!("File not found: {}", path);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_edit(
    path: &str,
    op: edit::EditOp,
    line_range: Option<&str>,
    after: Option<u32>,
    content: Option<&str>,
    content_file: Option<&PathBuf>,
    if_hash: Option<&str>,
    dry_run: bool,
    cli: &Cli,
) -> Result<()> {
    let (range_start, range_end) = if let Some(range) = line_range {
        parse_line_range(range)?
    } else {
        (None, None)
    };

    let edit_content = if let Some(path) = content_file {
        Some(std::fs::read_to_string(path)?)
    } else {
        content.map(ToString::to_string)
    };

    let request = edit::EditRequest {
        path: PathBuf::from(path),
        op,
        range_start,
        range_end,
        after,
        content: edit_content,
        if_hash: if_hash.map(ToString::to_string),
        dry_run,
    };

    let result = edit::apply_edit(&request)?;

    if dry_run {
        if cli.json {
            return print_json(json!({
                "path": path,
                "op": edit_op_str(op),
                "dry_run": true,
                "changed": result.changed,
                "old_hash": format!("{:x}", result.old_hash),
                "new_hash": format!("{:x}", result.new_hash),
                "line_count": result.line_count,
                "preview": result.preview,
            }));
        }
        println!("{}", result.preview);
        println!("old_hash:{:x}", result.old_hash);
        println!("new_hash:{:x}", result.new_hash);
        return Ok(());
    }

    if result.changed {
        let mut engine = load_engine(cli);
        engine.index_edited_file(path, &result.new_content, store_op(op));
        let snap_path = graph_path(cli);
        snapshot::write_snapshot(&engine, &snap_path)?;
        if cli.json {
            return print_json(json!({
                "path": path,
                "op": edit_op_str(op),
                "dry_run": false,
                "changed": true,
                "hash": format!("{:x}", result.new_hash),
                "line_count": result.line_count,
                "graph": snap_path.display().to_string(),
                "change_sequence": engine.store().current_seq(),
            }));
        }
        println!(
            "edit applied: {} lines, hash:{:x}",
            result.line_count, result.new_hash
        );
        println!("Graph saved to {}", snap_path.display());
    } else {
        if cli.json {
            return print_json(json!({
                "path": path,
                "op": edit_op_str(op),
                "dry_run": false,
                "changed": false,
                "hash": format!("{:x}", result.new_hash),
                "line_count": result.line_count,
            }));
        }
        println!("edit unchanged: hash:{:x}", result.new_hash);
    }

    Ok(())
}

fn store_op(op: edit::EditOp) -> store::Op {
    match op {
        edit::EditOp::Replace => store::Op::Replace,
        edit::EditOp::Insert => store::Op::Insert,
        edit::EditOp::Delete => store::Op::Delete,
    }
}

fn cmd_glob(pattern: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let results = engine.glob_files(pattern);

    if cli.json {
        return print_json(json!({
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

fn cmd_ls(path: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let entries = engine.list_dir(path);

    if cli.json {
        return print_json(json!({
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

fn cmd_status(cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let snap_path = graph_path(cli);
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
        return print_json(json!({
            "files_indexed": engine.file_count(),
            "symbols_indexed": engine.symbol_index_count(),
            "unique_words_indexed": engine.word_index_count(),
            "word_indexed_files": engine.word_index_file_count(),
            "seq": engine.store().current_seq(),
            "change_history_persisted": false,
            "graph": graph,
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

fn cmd_watch(path: &str, debounce_ms: u64, cli: &Cli) -> Result<()> {
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
    if !cli.no_graph {
        let snap_path = graph_path(cli);
        if snap_path.exists() {
            if let Ok(count) = snapshot::load_snapshot_into_engine(&mut engine, &snap_path) {
                eprintln!("Loaded {} files from graph", count);
            }
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

                    let snap_path = graph_path(cli);
                    if let Err(e) = snapshot::write_snapshot(&engine, &snap_path) {
                        eprintln!("Warning: Failed to save graph: {}", e);
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

fn parse_line_range(range: &str) -> Result<(Option<u32>, Option<u32>)> {
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

fn cmd_query(pipeline: &[String], cli: &Cli) -> Result<()> {
    let engine = load_engine(cli);
    let pipeline_str = pipeline.join(" ");
    let text = pipeline::run(&engine, &pipeline_str);
    if cli.json {
        return print_json(json!({
            "pipeline": pipeline_str,
            "text": text,
            "structured": false,
            "note": "Pipeline currently returns text because each stage can change result shape."
        }));
    }
    println!("{}", text);
    Ok(())
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn rich_results_json(results: &[engine::RichSearchResult]) -> Vec<serde_json::Value> {
    results
        .iter()
        .map(|result| {
            json!({
                "path": &result.path,
                "line": result.line_num,
                "line_num": result.line_num,
                "text": &result.line_text,
                "line_text": &result.line_text,
                "scope": result.scope.as_ref().map(|scope| json!({
                    "name": &scope.name,
                    "kind": scope.kind.to_string(),
                    "line_start": scope.line_start,
                    "line_end": scope.line_end,
                    "detail": &scope.detail,
                })),
            })
        })
        .collect()
}

fn edit_op_str(op: edit::EditOp) -> &'static str {
    match op {
        edit::EditOp::Replace => "replace",
        edit::EditOp::Insert => "insert",
        edit::EditOp::Delete => "delete",
    }
}

fn format_unix_ms_utc(ms: u64) -> String {
    if ms == 0 {
        return "unknown".to_string();
    }
    let seconds = (ms / 1000) as i64;
    let millis = ms % 1000;
    let (year, month, day, hour, minute, second) = unix_seconds_to_utc(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn unix_seconds_to_utc(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = (seconds_of_day / 3600) as u32;
    let minute = ((seconds_of_day % 3600) / 60) as u32;
    let second = (seconds_of_day % 60) as u32;
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month as u32, day as u32)
}
