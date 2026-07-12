use super::args::*;
use super::response::ToolOutput;
use super::server::McpServer;
use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::engine::{FileFilterOptions, SearchOptions, WordSearchOptions};
use crate::output::{
    format_unix_ms_utc, rich_results_json, word_result_kind_facets, word_result_path_facets,
};
use crate::project_path::{normalize_project_path, project_target_path, PathMode};

const MAX_RETRIEVAL_RESULTS: usize = 200;

impl McpServer {
    pub(super) fn tool_map(&self, args: &Value) -> ToolOutput {
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(MAX_RETRIEVAL_RESULTS)
            .min(MAX_RETRIEVAL_RESULTS);
        let filters = FileFilterOptions {
            path_prefix: opt_str(args, "path")
                .filter(|path| !path.is_empty())
                .map(ToString::to_string),
            path_glob: opt_str(args, "path_glob").map(ToString::to_string),
            language: opt_str(args, "language").map(ToString::to_string),
            min_lines: opt_u32(args, "min_lines"),
            max_lines: opt_u32(args, "max_lines"),
            max_results: Some(limit),
        };
        let (files, total, truncated) = self.engine.filtered_files(&filters);
        ToolOutput::new(json!({
            "count": files.len(),
            "total": total,
            "limit": limit,
            "truncated": truncated,
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
        }))
    }

    pub(super) fn tool_list(&self, path: &str) -> ToolOutput {
        let entries = self.engine.list_dir(path);
        if entries.is_empty() {
            return ToolOutput::new(json!({"path": path, "count": 0, "entries": []}));
        }

        let mut structured_entries = Vec::new();
        for (name, meta) in entries {
            if let Some(meta) = meta {
                structured_entries.push(json!({
                    "name": name,
                    "kind": "file",
                    "language": meta.language.as_str(),
                    "line_count": meta.line_count,
                    "byte_size": meta.byte_size,
                    "symbol_count": meta.symbol_count,
                    "modified_ms": meta.modified_ms,
                    "modified_utc": format_unix_ms_utc(meta.modified_ms),
                }));
            } else {
                structured_entries.push(json!({"name": name, "kind": "directory"}));
            }
        }
        ToolOutput::new(
            json!({"path": path, "count": structured_entries.len(), "entries": structured_entries}),
        )
    }

    pub(super) fn tool_glob(&self, pattern: &str) -> Result<ToolOutput> {
        let max = 200usize;
        let mut results = self.engine.glob_files(pattern);
        let total = results.len();
        let truncated = total > max;
        results.truncate(max);
        Ok(ToolOutput::new(json!({
            "pattern": pattern,
            "count": results.len(),
            "total": total,
            "limit": max,
            "truncated": truncated,
            "paths": results,
        })))
    }

    pub(super) fn tool_find_path(&self, query: &str, limit: usize) -> Result<ToolOutput> {
        let results = self.engine.fuzzy_find(query, limit);
        Ok(ToolOutput::new(json!({
            "query": query,
            "count": results.len(),
            "limit": limit,
            "results": results.into_iter().map(|(path, score)| json!({
                "path": path,
                "score": score,
            })).collect::<Vec<_>>()
        })))
    }

    pub(super) fn tool_outline(&self, path: &str) -> Result<ToolOutput> {
        let path = match normalize_project_path(&self.root, path, PathMode::Existing) {
            Ok(path) => path,
            Err(_) if !project_target_path(&self.root, path).exists() => {
                bail!("file not found: {path}");
            }
            Err(err) => return Err(err),
        };
        let Some(outline) = self.engine.get_outline(&path) else {
            bail!("file not found: {path}");
        };
        let unresolved_imports = self.engine.get_unresolved_imports(&path);

        Ok(ToolOutput::new(json!({
            "path": path,
            "language": outline.language.as_str(),
            "line_count": outline.line_count,
            "byte_size": outline.byte_size,
            "symbol_count": outline.symbols.len(),
            "imports": outline.imports,
            "unresolved_imports": unresolved_imports,
            "symbols": outline.symbols,
        })))
    }

    pub(super) fn tool_find_symbol(&self, name: &str) -> Result<ToolOutput> {
        let results = self.engine.find_symbol(name);
        Ok(ToolOutput::new(
            json!({"name": name, "count": results.len(), "results": results}),
        ))
    }

    pub(super) fn tool_symbol_search(&self, args: &Value) -> Result<ToolOutput> {
        let query = req_any_str(args, &["query", "name"])?;
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(20)
            .min(MAX_RETRIEVAL_RESULTS);
        let results = self.engine.fuzzy_symbols(query, limit);
        Ok(ToolOutput::new(json!({
            "query": query,
            "count": results.len(),
            "limit": limit,
            "results": results,
        })))
    }
}

impl McpServer {
    pub(super) fn tool_find_word(&self, args: &Value) -> Result<ToolOutput> {
        let word = req_any_str(args, &["word", "query"])?;
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(50)
            .clamp(1, MAX_RETRIEVAL_RESULTS);
        let options = WordSearchOptions {
            path_prefix: opt_str(args, "path_prefix")
                .or_else(|| opt_str(args, "path"))
                .map(ToString::to_string),
            path_glob: opt_str(args, "path_glob").map(ToString::to_string),
        };
        let all_results = self.engine.search_word_with_options(word, &options);
        let total = all_results.len();
        let cursor = opt_usize(args, "cursor").unwrap_or(0).min(total);
        let end = cursor.saturating_add(limit).min(total);
        let results = all_results[cursor..end].to_vec();
        let next_cursor = (end < total).then_some(end);
        Ok(ToolOutput::new(json!({
            "query": word,
            "count": results.len(),
            "total": total,
            "limit": limit,
            "cursor": cursor,
            "truncated": next_cursor.is_some(),
            "next_cursor": next_cursor,
            "filters": {
                "path_prefix": options.path_prefix,
                "path_glob": options.path_glob,
            },
            "facets": word_result_path_facets(&all_results),
            "kind_facets": word_result_kind_facets(&all_results),
            "results": results,
        })))
    }

    pub(super) fn tool_search(&self, args: &Value) -> Result<ToolOutput> {
        let query = req_str(args, "query")?;
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(20)
            .min(MAX_RETRIEVAL_RESULTS);
        let options = SearchOptions {
            max_results: limit.saturating_add(1),
            regex: opt_bool(args, "regex").unwrap_or(false),
            scope: opt_bool(args, "scope").unwrap_or(false),
            compact: opt_bool(args, "compact").unwrap_or(false),
            paths_only: opt_bool(args, "paths_only").unwrap_or(false),
            path_glob: opt_str(args, "path_glob").map(ToString::to_string),
        };
        let results = self
            .engine
            .search_rich(query, &options)
            .map_err(anyhow::Error::msg)?;

        let truncated = results.len() > limit;
        let results: Vec<_> = results.into_iter().take(limit).collect();
        if results.is_empty() {
            return Ok(ToolOutput::new(json!({
                "query": query,
                "count": 0,
                "limit": limit,
                "truncated": false,
                "results": []
            })));
        }

        Ok(ToolOutput::new(json!({
            "query": query,
            "count": results.len(),
            "limit": limit,
            "truncated": truncated,
            "regex": options.regex,
            "scope": options.scope,
            "compact": options.compact,
            "paths_only": options.paths_only,
            "path_glob": options.path_glob,
            "results": rich_results_json(&results),
        })))
    }

    pub(super) fn tool_find_callers(&self, name: &str) -> Result<ToolOutput> {
        let results = self.engine.find_callers(name, 30);
        Ok(ToolOutput::new(
            json!({"name": name, "count": results.len(), "limit": 30, "results": results}),
        ))
    }

    pub(super) fn tool_brief(&self, args: &Value) -> Result<ToolOutput> {
        let task = req_any_str(args, &["task", "query"])?;
        let options = crate::engine::ContextOptions {
            max_results: opt_usize(args, "max_results")
                .or_else(|| opt_usize(args, "max"))
                .unwrap_or(10)
                .min(MAX_RETRIEVAL_RESULTS),
            path_prefix: opt_str(args, "path_prefix")
                .or_else(|| opt_str(args, "path"))
                .map(ToString::to_string),
            path_glob: opt_str(args, "path_glob").map(ToString::to_string),
            language: opt_str(args, "language").map(ToString::to_string),
        };
        let details = self
            .engine
            .build_context_details_with_options(task, &options);
        Ok(ToolOutput::new(json!(details)))
    }

    pub(super) fn tool_trace_deps(&self, args: &Value) -> Result<ToolOutput> {
        let path = normalize_project_path(&self.root, req_str(args, "path")?, PathMode::Existing)?;
        let direction = opt_str(args, "direction").unwrap_or("imported_by");
        let transitive = opt_bool(args, "transitive").unwrap_or(false);
        let deps = match (direction, transitive) {
            ("depends_on", true) => self.engine.get_transitive_depends_on(&path),
            ("depends_on", false) => self.engine.get_depends_on(&path),
            ("imported_by", true) => self.engine.get_transitive_imported_by(&path),
            ("imported_by", false) => self.engine.get_imported_by(&path),
            _ => bail!("direction must be imported_by or depends_on"),
        };
        let unresolved_imports = if direction == "depends_on" {
            self.engine.get_unresolved_imports(&path)
        } else {
            Vec::new()
        };

        Ok(ToolOutput::new(json!({
            "path": path,
            "direction": direction,
            "transitive": transitive,
            "count": deps.len(),
            "dependencies": deps,
            "unresolved_imports": unresolved_imports,
        })))
    }
}
