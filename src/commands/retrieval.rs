use crate::cli::Cli;
use anyhow::Result;
use lexa::engine::{ContextOptions, FileFilterOptions, SearchOptions, WordSearchOptions};
use lexa::output::{
    format_unix_ms_utc, rich_results_json, word_result_kind_facets, word_result_path_facets,
};
use lexa::project_path::{normalize_project_path, project_target_path, PathMode};
use serde_json::json;

use super::shared::*;

pub(crate) fn cmd_search(query: &str, options: SearchOptions, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;

    let results = match engine.search_rich(query, &options) {
        Ok(results) => results,
        Err(e) => {
            if cli.json {
                return print_agent_result(json!({
                    "error": "search_failed",
                    "message": e.to_string(),
                }));
            }
            eprintln!("Error: {}", e);
            return Ok(());
        }
    };

    if cli.json {
        return print_agent_result(json!({
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

pub(crate) fn cmd_outline(path: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let root = std::env::current_dir()?;
    let path = match normalize_project_path(&root, path, PathMode::Existing) {
        Ok(path) => path,
        Err(_) if !project_target_path(&root, path).exists() => {
            if cli.json {
                return print_agent_result(json!({
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
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    match engine.get_outline(&path) {
        Some(outline) => {
            let unresolved_imports = engine.get_unresolved_imports(&path);
            if cli.json {
                return print_agent_result(json!({
                    "path": path,
                    "language": outline.language.as_str(),
                    "line_count": outline.line_count,
                    "byte_size": outline.byte_size,
                    "symbol_count": outline.symbols.len(),
                    "imports": &outline.imports,
                    "unresolved_imports": unresolved_imports,
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

            if !unresolved_imports.is_empty() {
                println!("Unresolved local imports:");
                for import in &unresolved_imports {
                    let line = import
                        .line_start
                        .map(|line| format!("L{line}: "))
                        .unwrap_or_default();
                    println!("  {}{}", line, import.import);
                }
                println!();
            }

            if !outline.symbols.is_empty() {
                println!("Symbols:");
                for sym in outline
                    .symbols
                    .iter()
                    .filter(|sym| sym.kind != lexa::types::SymbolKind::Import)
                {
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
                return print_agent_result(json!({
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

pub(crate) fn cmd_symbol_search(query: &str, max: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let results = engine.fuzzy_symbols(query, max);

    if cli.json {
        return print_agent_result(json!({
            "query": query,
            "count": results.len(),
            "limit": max,
            "results": results,
        }));
    }

    if results.is_empty() {
        println!("No symbols found matching '{}'", query);
        return Ok(());
    }

    println!("{} symbol(s) matching '{}':", results.len(), query);
    for result in &results {
        let detail = result.detail.as_deref().unwrap_or("");
        let detail_str = if detail.is_empty() {
            String::new()
        } else {
            format!(" {}", detail)
        };
        println!(
            "  {:.2}  {}:{}-{} {} {}{}",
            result.score,
            result.path,
            result.line_start,
            result.line_end,
            result.kind,
            result.name,
            detail_str
        );
    }

    Ok(())
}

pub(crate) fn cmd_symbol(name: &str, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let results = engine.find_symbol(name);

    if cli.json {
        return print_agent_result(
            json!({"name": name, "count": results.len(), "results": results}),
        );
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

pub(crate) fn cmd_word(
    word: &str,
    limit: usize,
    cursor: usize,
    path_prefix: Option<&str>,
    path_glob: Option<&str>,
    cli: &Cli,
) -> Result<()> {
    let engine = load_engine(cli)?;
    let limit = limit.max(1);
    let options = WordSearchOptions {
        path_prefix: path_prefix.map(ToString::to_string),
        path_glob: path_glob.map(ToString::to_string),
    };
    let all_results = engine.search_word_with_options(word, &options);
    let total = all_results.len();
    let start = cursor.min(total);
    let end = start.saturating_add(limit).min(total);
    let results = all_results[start..end].to_vec();
    let next_cursor = (end < total).then_some(end);

    if cli.json {
        return print_agent_result(json!({
            "word": word,
            "count": results.len(),
            "total": total,
            "limit": limit,
            "cursor": start,
            "truncated": next_cursor.is_some(),
            "next_cursor": next_cursor,
            "filters": {
                "path_prefix": options.path_prefix,
                "path_glob": options.path_glob,
            },
            "facets": word_result_path_facets(&all_results),
            "kind_facets": word_result_kind_facets(&all_results),
            "results": results,
        }));
    }

    if all_results.is_empty() {
        println!("No occurrences of '{}'", word);
        return Ok(());
    }

    println!(
        "{} occurrence(s) of '{}' (showing {} from cursor {}):",
        total,
        word,
        results.len(),
        start
    );
    for result in &results {
        println!(
            "  {}:{}: {}",
            result.path, result.line_num, result.line_text
        );
    }
    if let Some(next_cursor) = next_cursor {
        println!("Next: lexa word-refs {word} --cursor {next_cursor}");
    }

    Ok(())
}

pub(crate) fn cmd_find(pattern: &str, max: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let results = engine.fuzzy_find(pattern, max);

    if cli.json {
        return print_agent_result(json!({
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

pub(crate) fn cmd_tree(filters: FileFilterOptions, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let (files, total, truncated) = engine.filtered_files(&filters);

    if cli.json {
        return print_agent_result(json!({
            "count": files.len(),
            "total": total,
            "truncated": truncated,
            "limit": filters.max_results,
            "filters": {
                "path_prefix": filters.path_prefix,
                "path_glob": filters.path_glob,
                "language": filters.language,
                "min_lines": filters.min_lines,
                "max_lines": filters.max_lines,
            },
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
        println!("No indexed files match filters");
    } else {
        for (path, meta) in &files {
            println!(
                "{:<60} {:>8} {:>6}L {:>4} sym",
                path,
                meta.language.as_str(),
                meta.line_count,
                meta.symbol_count
            );
        }
        if truncated {
            println!("showing {} of {} matched files", files.len(), total);
        }
    }

    Ok(())
}

pub(crate) fn cmd_deps(path: &str, reverse: bool, transitive: bool, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let root = std::env::current_dir()?;
    let path = normalize_project_path(&root, path, PathMode::Existing)?;

    let deps = if transitive {
        if reverse {
            engine.get_transitive_imported_by(&path)
        } else {
            engine.get_transitive_depends_on(&path)
        }
    } else if reverse {
        engine.get_imported_by(&path)
    } else {
        engine.get_depends_on(&path)
    };
    let unresolved_imports = if reverse {
        Vec::new()
    } else {
        engine.get_unresolved_imports(&path)
    };

    let label = if reverse { "imported by" } else { "depends on" };
    let transitive_label = if transitive { " (transitive)" } else { "" };

    if cli.json {
        return print_agent_result(json!({
            "path": path,
            "direction": if reverse { "imported_by" } else { "depends_on" },
            "transitive": transitive,
            "count": deps.len(),
            "dependencies": deps,
            "unresolved_imports": unresolved_imports,
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
    if !unresolved_imports.is_empty() {
        println!("Unresolved local import(s):");
        for import in &unresolved_imports {
            let line = import
                .line_start
                .map(|line| format!("L{line}: "))
                .unwrap_or_default();
            println!("  {}{}", line, import.import);
        }
    }

    Ok(())
}

pub(crate) fn cmd_hot(limit: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let files = engine.get_hot_files(limit);

    if cli.json {
        return print_agent_result(json!({
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

pub(crate) fn cmd_callers(name: &str, max: usize, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let results = engine.find_callers(name, max);

    if cli.json {
        return print_agent_result(json!({
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

pub(crate) fn cmd_context(task: &str, options: ContextOptions, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let details = engine.build_context_details_with_options(task, &options);
    if cli.json {
        return print_agent_result(json!(details));
    }
    let context = engine.build_context_with_options(task, &options);
    println!("{}", context);
    Ok(())
}

pub(crate) fn cmd_changes(since: u64, cli: &Cli) -> Result<()> {
    let engine = load_engine(cli)?;
    let changes = engine.get_changes(since);

    if cli.json {
        return print_agent_result(json!({
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
