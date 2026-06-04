use anyhow::{bail, Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

use crate::audit::{self, AuditOptions};
use crate::edit::{self, EditOp};
use crate::engine::{Engine, FileFilterOptions, SearchOptions};
use crate::freshness;
use crate::output::{format_unix_ms_utc, rich_results_json};
use crate::project_path::{normalize_project_path, project_target_path, PathMode};
use crate::snapshot;
use crate::store;

const DEFAULT_MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub struct McpServer {
    engine: Engine,
    root: PathBuf,
    graph_path: PathBuf,
    persist_graph: bool,
    include_structured_content: bool,
    watcher: Option<RuntimeWatcher>,
}

struct RuntimeWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
}

struct ToolOutput {
    text: String,
    structured: Value,
}

struct McpMessage {
    body: Vec<u8>,
    framing: StdioFraming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StdioFraming {
    ContentLength,
    NewlineDelimited,
}

impl ToolOutput {
    fn new(text: String, structured: Value) -> Self {
        Self { text, structured }
    }
}

impl McpServer {
    pub fn new(
        engine: Engine,
        root: PathBuf,
        graph_path: PathBuf,
        persist_graph: bool,
        include_structured_content: bool,
    ) -> Self {
        Self {
            engine,
            root,
            graph_path,
            persist_graph,
            include_structured_content,
            watcher: None,
        }
    }

    pub fn enable_watcher(&mut self, debounce_ms: u64) -> Result<()> {
        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(
            tx,
            notify::Config::default().with_poll_interval(Duration::from_millis(debounce_ms)),
        )?;
        watcher.watch(&self.root, RecursiveMode::Recursive)?;
        eprintln!("Watching {} for MCP graph freshness", self.root.display());
        self.watcher = Some(RuntimeWatcher {
            _watcher: watcher,
            rx,
        });
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut reader = BufReader::new(stdin.lock());
        let mut writer = stdout.lock();

        while let Some(message) = read_message(&mut reader)? {
            let request: Value = match serde_json::from_slice(&message.body) {
                Ok(value) => value,
                Err(err) => {
                    write_response(
                        &mut writer,
                        message.framing,
                        &json_rpc_error(None, -32700, &err.to_string()),
                    )?;
                    continue;
                }
            };

            self.refresh_from_watcher()?;

            let id = request.get("id").cloned();
            let Some(method) = request.get("method").and_then(Value::as_str) else {
                if id.is_some() {
                    write_response(
                        &mut writer,
                        message.framing,
                        &json_rpc_error(id, -32600, "missing JSON-RPC method"),
                    )?;
                }
                continue;
            };

            let Some(response) = self.handle(method, id, request.get("params")) else {
                continue;
            };
            write_response(&mut writer, message.framing, &response)?;
        }

        Ok(())
    }

    fn refresh_from_watcher(&mut self) -> Result<()> {
        let Some(watcher) = &self.watcher else {
            return Ok(());
        };

        let mut paths = Vec::new();
        loop {
            match watcher.rx.try_recv() {
                Ok(Ok(event)) => {
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) {
                        paths.extend(event.paths);
                    }
                }
                Ok(Err(err)) => eprintln!("Watch error: {err}"),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if paths.is_empty() {
            return Ok(());
        }

        let summary = freshness::refresh_paths(&mut self.engine, &self.root, paths)?;
        if summary.changed() {
            if self.persist_graph {
                snapshot::write_snapshot(&self.engine, &self.graph_path)?;
            }
            eprintln!(
                "MCP graph refreshed: {} indexed, {} removed",
                summary.indexed, summary.removed
            );
        }

        Ok(())
    }

    fn handle(&mut self, method: &str, id: Option<Value>, params: Option<&Value>) -> Option<Value> {
        match method {
            "initialize" => id.map(|id| {
                let protocol_version = requested_protocol_version(params);
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": protocol_version,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": "lexa", "version": env!("CARGO_PKG_VERSION") }
                    }
                })
            }),
            "notifications/initialized" => None,
            "tools/list" => id.map(|id| {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": tools() }
                })
            }),
            "tools/call" => {
                let id = id?;
                let Some(params) = params else {
                    return Some(json_rpc_error(Some(id), -32602, "missing params"));
                };
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(json_rpc_error(Some(id), -32602, "missing tool name"));
                };
                let args = params.get("arguments").unwrap_or(&Value::Null);
                let result = self.call_tool(name, args);
                Some(tool_response(id, result, self.include_structured_content))
            }
            "ping" => id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": {} })),
            _ => id.map(|id| json_rpc_error(Some(id), -32601, "method not found")),
        }
    }

    fn call_tool(&mut self, name: &str, args: &Value) -> Result<ToolOutput> {
        match name {
            "files" => Ok(self.tool_map(args)),
            "list" => Ok(self.tool_list(opt_str(args, "path").unwrap_or(""))),
            "glob" => self.tool_glob(req_str(args, "pattern")?),
            "path_search" => self.tool_find_path(
                req_any_str(args, &["query", "path", "pattern", "name"])?,
                opt_usize(args, "max_results")
                    .or_else(|| opt_usize(args, "max"))
                    .unwrap_or(20),
            ),
            "outline" => self.tool_outline(req_str(args, "path")?),
            "symbol_defs" => self.tool_find_symbol(req_any_str(args, &["name", "query"])?),
            "symbol_search" => self.tool_symbol_search(args),
            "word_refs" => self.tool_find_word(req_any_str(args, &["word", "query"])?),
            "text_search" => self.tool_search(args),
            "callers" => self.tool_find_callers(req_any_str(args, &["name", "query"])?),
            "brief" => self.tool_brief(args),
            "trace_deps" => self.tool_trace_deps(args),
            "read" => self.tool_read(args),
            "patch" => self.tool_patch(args),
            "create" => self.tool_create(args),
            "changes" => Ok(self.tool_changes(opt_u64(args, "since").unwrap_or(0))),
            "recent" => Ok(self.tool_recent(opt_usize(args, "limit").unwrap_or(10))),
            "status" => Ok(self.tool_status()),
            "reindex" => self.tool_reindex(),
            "clear_index" => self.tool_clear_index(),
            "audit" => self.tool_audit(args),
            "pipeline" => self.tool_pipeline(args),
            _ => bail!("unknown tool: {name}"),
        }
    }

    fn tool_map(&self, args: &Value) -> ToolOutput {
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(200);
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
        let text = files
            .iter()
            .map(|(path, meta)| {
                format!(
                    "{:<60} {:>8} {:>6}L {:>4} sym",
                    path,
                    meta.language.as_str(),
                    meta.line_count,
                    meta.symbol_count
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        ToolOutput::new(
            text,
            json!({
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
            }),
        )
    }

    fn tool_list(&self, path: &str) -> ToolOutput {
        let entries = self.engine.list_dir(path);
        if entries.is_empty() {
            return ToolOutput::new(
                format!("No files in '{path}'"),
                json!({"path": path, "count": 0, "entries": []}),
            );
        }

        let mut out = String::new();
        let mut structured_entries = Vec::new();
        for (name, meta) in entries {
            if let Some(meta) = meta {
                out.push_str(&format!(
                    "{:<60} {:>8} {:>6}L {:>4} sym\n",
                    name,
                    meta.language.as_str(),
                    meta.line_count,
                    meta.symbol_count
                ));
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
                out.push_str(&format!("{name}/\n"));
                structured_entries.push(json!({"name": name, "kind": "directory"}));
            }
        }
        ToolOutput::new(
            out,
            json!({"path": path, "count": structured_entries.len(), "entries": structured_entries}),
        )
    }

    fn tool_glob(&self, pattern: &str) -> Result<ToolOutput> {
        let max = 200usize;
        let mut results = self.engine.glob_files(pattern);
        let total = results.len();
        let truncated = total > max;
        results.truncate(max);
        let text = if results.is_empty() {
            format!("No files match '{pattern}'")
        } else {
            results.join("\n")
        };
        Ok(ToolOutput::new(
            text,
            json!({
                "pattern": pattern,
                "count": results.len(),
                "total": total,
                "limit": max,
                "truncated": truncated,
                "paths": results,
            }),
        ))
    }

    fn tool_find_path(&self, query: &str, limit: usize) -> Result<ToolOutput> {
        let results = self.engine.fuzzy_find(query, limit);
        let text = if results.is_empty() {
            format!("No files found matching '{query}'")
        } else {
            results
                .iter()
                .map(|(path, score)| format!("{path}  score:{score:.1}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput::new(
            text,
            json!({
                "query": query,
                "count": results.len(),
                "limit": limit,
                "results": results.into_iter().map(|(path, score)| json!({
                    "path": path,
                    "score": score,
                })).collect::<Vec<_>>()
            }),
        ))
    }

    fn tool_outline(&self, path: &str) -> Result<ToolOutput> {
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

        let mut out = format!(
            "{} ({} lines, {} symbols)\nLanguage: {}\n",
            path,
            outline.line_count,
            outline.symbols.len(),
            outline.language
        );
        if !outline.imports.is_empty() {
            out.push_str("\nImports:\n");
            for import in &outline.imports {
                out.push_str(&format!("  {import}\n"));
            }
        }
        if !unresolved_imports.is_empty() {
            out.push_str("\nUnresolved local imports:\n");
            for import in &unresolved_imports {
                let line = import
                    .line_start
                    .map(|line| format!("L{line}: "))
                    .unwrap_or_default();
                out.push_str(&format!("  {line}{}\n", import.import));
            }
        }
        if !outline.symbols.is_empty() {
            out.push_str("\nSymbols:\n");
            for sym in outline
                .symbols
                .iter()
                .filter(|sym| sym.kind != crate::types::SymbolKind::Import)
            {
                let detail = sym.detail.as_deref().unwrap_or("");
                let detail_str = if detail.is_empty() {
                    String::new()
                } else {
                    format!(" {detail}")
                };
                out.push_str(&format!(
                    "  L{:<5} {:<12} {}{}\n",
                    sym.line_start, sym.kind, sym.name, detail_str
                ));
            }
        }
        Ok(ToolOutput::new(
            out,
            json!({
                "path": path,
                "language": outline.language.as_str(),
                "line_count": outline.line_count,
                "byte_size": outline.byte_size,
                "symbol_count": outline.symbols.len(),
                "imports": outline.imports,
                "unresolved_imports": unresolved_imports,
                "symbols": outline.symbols,
            }),
        ))
    }

    fn tool_find_symbol(&self, name: &str) -> Result<ToolOutput> {
        let results = self.engine.find_symbol(name);
        let text = if results.is_empty() {
            format!("No symbols found for '{name}'")
        } else {
            results
                .iter()
                .map(|result| {
                    let detail = result.symbol.detail.clone().unwrap_or_default();
                    let detail = if detail.is_empty() {
                        String::new()
                    } else {
                        format!(" {detail}")
                    };
                    format!(
                        "{}:{}-{} {} {}{}",
                        result.path,
                        result.symbol.line_start,
                        result.symbol.line_end,
                        result.symbol.kind,
                        result.symbol.name,
                        detail
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput::new(
            text,
            json!({"name": name, "count": results.len(), "results": results}),
        ))
    }

    fn tool_symbol_search(&self, args: &Value) -> Result<ToolOutput> {
        let query = req_any_str(args, &["query", "name"])?;
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(20);
        let results = self.engine.fuzzy_symbols(query, limit);
        let text = if results.is_empty() {
            format!("No symbols found matching '{query}'")
        } else {
            results
                .iter()
                .map(|result| {
                    let detail = result.detail.as_deref().unwrap_or("");
                    let detail_str = if detail.is_empty() {
                        String::new()
                    } else {
                        format!(" {detail}")
                    };
                    format!(
                        "{:.2}  {}:{}-{} {} {}{}",
                        result.score,
                        result.path,
                        result.line_start,
                        result.line_end,
                        result.kind,
                        result.name,
                        detail_str
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolOutput::new(
            text,
            json!({
                "query": query,
                "count": results.len(),
                "limit": limit,
                "results": results,
            }),
        ))
    }

    fn tool_find_word(&self, word: &str) -> Result<ToolOutput> {
        let results = self.engine.search_word(word);
        Ok(ToolOutput::new(
            render_search_results(word, &results),
            json!({"query": word, "count": results.len(), "results": results}),
        ))
    }

    fn tool_search(&self, args: &Value) -> Result<ToolOutput> {
        let query = req_str(args, "query")?;
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(20);
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
            return Ok(ToolOutput::new(
                format!("No results found for '{query}'"),
                json!({
                    "query": query,
                    "count": 0,
                    "limit": limit,
                    "truncated": false,
                    "results": []
                }),
            ));
        }

        let mut out = format!("{} results for '{query}':\n", results.len());
        for result in &results {
            if options.paths_only {
                out.push_str(&format!("{}:{}\n", result.path, result.line_num));
            } else if let Some(scope) = &result.scope {
                out.push_str(&format!(
                    "{}:{}: {}  [{} {}:{}-{}]\n",
                    result.path,
                    result.line_num,
                    result.line_text,
                    scope.kind,
                    scope.name,
                    scope.line_start,
                    scope.line_end
                ));
            } else {
                out.push_str(&format!(
                    "{}:{}: {}\n",
                    result.path, result.line_num, result.line_text
                ));
            }
        }
        Ok(ToolOutput::new(
            out,
            json!({
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
            }),
        ))
    }

    fn tool_find_callers(&self, name: &str) -> Result<ToolOutput> {
        let results = self.engine.find_callers(name, 30);
        Ok(ToolOutput::new(
            render_search_results(name, &results),
            json!({"name": name, "count": results.len(), "limit": 30, "results": results}),
        ))
    }

    fn tool_brief(&self, args: &Value) -> Result<ToolOutput> {
        let task = req_any_str(args, &["task", "query"])?;
        let options = crate::engine::ContextOptions {
            max_results: opt_usize(args, "max_results")
                .or_else(|| opt_usize(args, "max"))
                .unwrap_or(10),
            path_prefix: opt_str(args, "path_prefix")
                .or_else(|| opt_str(args, "path"))
                .map(ToString::to_string),
            path_glob: opt_str(args, "path_glob").map(ToString::to_string),
            language: opt_str(args, "language").map(ToString::to_string),
        };
        let text = self.engine.build_context_with_options(task, &options);
        let details = self
            .engine
            .build_context_details_with_options(task, &options);
        Ok(ToolOutput::new(text, json!(details)))
    }

    fn tool_trace_deps(&self, args: &Value) -> Result<ToolOutput> {
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

        let text = if deps.is_empty() {
            format!("No {direction} dependencies for {path}")
        } else {
            deps.join("\n")
        };
        Ok(ToolOutput::new(
            text,
            json!({
                "path": path,
                "direction": direction,
                "transitive": transitive,
                "count": deps.len(),
                "dependencies": deps,
                "unresolved_imports": unresolved_imports,
            }),
        ))
    }

    fn tool_read(&self, args: &Value) -> Result<ToolOutput> {
        let path = normalize_project_path(&self.root, req_str(args, "path")?, PathMode::Existing)?;
        let line_start = opt_u32(args, "line_start");
        let line_end = opt_u32(args, "line_end");
        let result = self
            .engine
            .read_file_rich(
                &path,
                line_start,
                line_end,
                opt_bool(args, "compact").unwrap_or(false),
                opt_str(args, "if_hash"),
            )
            .with_context(|| format!("file not found: {path}"))?;

        if result.unchanged {
            let hash = format!("{:x}", result.hash);
            return Ok(ToolOutput::new(
                format!("unchanged:{hash}"),
                json!({
                    "path": path,
                    "hash": hash,
                    "unchanged": true,
                    "line_start": line_start,
                    "line_end": line_end,
                    "content": ""
                }),
            ));
        }

        let hash = format!("{:x}", result.hash);
        let mut out = format!("hash:{hash}\n");
        out.push_str(&result.content);
        Ok(ToolOutput::new(
            out,
            json!({
                "path": path,
                "hash": hash,
                "unchanged": false,
                "line_start": line_start,
                "line_end": line_end,
                "compact": opt_bool(args, "compact").unwrap_or(false),
                "content": result.content,
            }),
        ))
    }

    fn tool_patch(&mut self, args: &Value) -> Result<ToolOutput> {
        let rel_path =
            normalize_project_path(&self.root, req_str(args, "path")?, PathMode::Existing)?;
        let abs_path = project_target_path(&self.root, &rel_path);
        let op = parse_edit_op(req_str(args, "op")?)?;

        let request = edit::EditRequest {
            path: abs_path,
            op,
            range_start: opt_u32(args, "range_start"),
            range_end: opt_u32(args, "range_end"),
            after: opt_u32(args, "after"),
            content: opt_str(args, "content").map(ToString::to_string),
            if_hash: opt_str(args, "if_hash").map(ToString::to_string),
            dry_run: opt_bool(args, "dry_run").unwrap_or(false),
        };

        let result = edit::apply_edit(&request)?;
        if request.dry_run {
            let text = format!(
                "{}\nold_hash:{:x}\nnew_hash:{:x}",
                result.preview, result.old_hash, result.new_hash
            );
            return Ok(ToolOutput::new(
                text,
                json!({
                    "path": rel_path,
                    "op": req_str(args, "op")?,
                    "dry_run": true,
                    "changed": result.changed,
                    "old_hash": format!("{:x}", result.old_hash),
                    "new_hash": format!("{:x}", result.new_hash),
                    "line_count": result.line_count,
                    "preview": result.preview,
                }),
            ));
        }

        if result.changed {
            self.engine
                .index_edited_file(&rel_path, &result.new_content, store_op(op));
            if self.persist_graph {
                snapshot::write_snapshot(&self.engine, &self.graph_path)?;
            }
            let hash = format!("{:x}", result.new_hash);
            Ok(ToolOutput::new(
                format!("patch applied: {} lines, hash:{hash}", result.line_count),
                json!({
                    "path": rel_path,
                    "op": req_str(args, "op")?,
                    "dry_run": false,
                    "changed": true,
                    "hash": hash,
                    "line_count": result.line_count,
                    "graph": self.graph_path.display().to_string(),
                    "change_sequence": self.engine.store().current_seq(),
                }),
            ))
        } else {
            let hash = format!("{:x}", result.new_hash);
            Ok(ToolOutput::new(
                format!("patch unchanged: hash:{hash}"),
                json!({
                    "path": rel_path,
                    "op": req_str(args, "op")?,
                    "dry_run": false,
                    "changed": false,
                    "hash": hash,
                    "line_count": result.line_count,
                }),
            ))
        }
    }

    fn tool_create(&mut self, args: &Value) -> Result<ToolOutput> {
        let rel_path =
            normalize_project_path(&self.root, req_str(args, "path")?, PathMode::Create)?;
        let abs_path = project_target_path(&self.root, &rel_path);
        let content = opt_str(args, "content").unwrap_or("").to_string();
        let overwrite = opt_bool(args, "overwrite").unwrap_or(false);
        let dry_run = opt_bool(args, "dry_run").unwrap_or(false);

        let request = edit::CreateRequest {
            path: abs_path,
            content: content.clone(),
            overwrite,
            dry_run,
        };
        let result = edit::create_file(&request)?;
        if !dry_run {
            self.engine
                .index_edited_file(&rel_path, &content, store::Op::Create);
            if self.persist_graph {
                snapshot::write_snapshot(&self.engine, &self.graph_path)?;
            }
        }

        let hash = format!("{:x}", result.hash);
        let text = if dry_run {
            format!("create dry-run: {} lines, hash:{hash}", result.line_count)
        } else {
            format!("file created: {} lines, hash:{hash}", result.line_count)
        };
        Ok(ToolOutput::new(
            text,
            json!({
                "path": rel_path,
                "op": "create",
                "dry_run": dry_run,
                "changed": result.changed,
                "hash": hash,
                "line_count": result.line_count,
                "byte_size": result.byte_size,
                "change_sequence": self.engine.store().current_seq(),
            }),
        ))
    }

    fn tool_changes(&self, since: u64) -> ToolOutput {
        let changes = self.engine.get_changes(since);
        if changes.is_empty() {
            return ToolOutput::new(
                format!("No changes since sequence {since} in this session"),
                json!({
                    "since": since,
                    "count": 0,
                    "change_history_persisted": false,
                    "note": "Change history is session-local and is not restored from graph snapshots.",
                    "changes": []
                }),
            );
        }

        let text = changes
            .iter()
            .map(|(path, seq, op)| format!("{path} (seq {seq}): {op}"))
            .collect::<Vec<_>>()
            .join("\n");
        ToolOutput::new(
            text,
            json!({
                "since": since,
                "count": changes.len(),
                "change_history_persisted": false,
                "note": "Change history is session-local and is not restored from graph snapshots.",
                "changes": changes.into_iter().map(|(path, seq, op)| json!({
                    "path": path,
                    "seq": seq,
                    "op": op,
                })).collect::<Vec<_>>()
            }),
        )
    }

    fn tool_recent(&self, limit: usize) -> ToolOutput {
        let files = self.engine.get_hot_files(limit);
        if files.is_empty() {
            return ToolOutput::new(
                "No files indexed".to_string(),
                json!({"count": 0, "limit": limit, "files": []}),
            );
        }

        let text = files
            .iter()
            .map(|(path, meta)| {
                format!(
                    "{}  {}L  {}",
                    format_unix_ms_utc(meta.modified_ms),
                    meta.line_count,
                    path
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        ToolOutput::new(
            text,
            json!({
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
            }),
        )
    }

    fn tool_status(&self) -> ToolOutput {
        let graph = graph_status(&self.graph_path);
        let text = format!(
            "files: {}\nseq: {} (session-local)\ngraph: {}\ngraph_exists: {}\nchange_history_persisted: false",
            self.engine.file_count(),
            self.engine.store().current_seq(),
            self.graph_path.display(),
            graph["exists"].as_bool().unwrap_or(false)
        )
        .to_string();
        ToolOutput::new(
            text,
            json!({
                "files_indexed": self.engine.file_count(),
                "symbols_indexed": self.engine.symbol_index_count(),
                "unique_words_indexed": self.engine.word_index_count(),
                "word_indexed_files": self.engine.word_index_file_count(),
                "seq": self.engine.store().current_seq(),
                "change_history_persisted": false,
                "graph": graph,
            }),
        )
    }

    fn tool_reindex(&mut self) -> Result<ToolOutput> {
        let mut engine = Engine::new(16384);
        let count = engine.index_project(&self.root);
        if self.persist_graph {
            snapshot::write_snapshot(&engine, &self.graph_path)?;
        }
        self.engine = engine;
        Ok(ToolOutput::new(
            format!("reindexed {count} files"),
            json!({
                "files_indexed": count,
                "symbols_indexed": self.engine.symbol_index_count(),
                "unique_words_indexed": self.engine.word_index_count(),
                "word_indexed_files": self.engine.word_index_file_count(),
                "graph": graph_status(&self.graph_path),
                "persisted": self.persist_graph,
            }),
        ))
    }

    fn tool_clear_index(&mut self) -> Result<ToolOutput> {
        let existed = self.graph_path.exists();
        if existed {
            std::fs::remove_file(&self.graph_path)
                .with_context(|| format!("failed to remove graph {}", self.graph_path.display()))?;
        }
        self.engine = Engine::new(16384);
        Ok(ToolOutput::new(
            if existed {
                format!(
                    "cleared index and removed graph {}",
                    self.graph_path.display()
                )
            } else {
                "cleared index; no graph file was present".to_string()
            },
            json!({
                "cleared": true,
                "graph_removed": existed,
                "graph": graph_status(&self.graph_path),
            }),
        ))
    }

    fn tool_audit(&self, args: &Value) -> Result<ToolOutput> {
        let max_results = opt_usize(args, "max_results").or_else(|| opt_usize(args, "max"));
        let config_path = opt_str(args, "config").map(PathBuf::from);
        let config = audit::load_audit_config(
            &self.root,
            config_path.as_deref(),
            opt_bool(args, "no_config").unwrap_or(false),
        )?;
        let includes = audit_includes(args)?;
        let scope = if let Some(base) = opt_str(args, "since") {
            audit::AuditScope::GitSince {
                base: base.to_string(),
                changed_files: audit::changed_files_since(&self.root, base)?,
            }
        } else {
            audit::AuditScope::Project
        };
        let report = audit::run_audit(
            &self.engine,
            AuditOptions {
                max_results,
                scope,
                config,
                includes,
            },
        );
        let text = audit::render_audit_report(&report);
        Ok(ToolOutput::new(text, json!(report)))
    }

    fn tool_pipeline(&self, args: &Value) -> Result<ToolOutput> {
        let pipeline_arg = opt_str(args, "pipeline");
        let steps_arg = args.get("steps").and_then(Value::as_array);

        let pipeline = if let Some(text) = pipeline_arg {
            text.to_string()
        } else if let Some(steps) = steps_arg {
            steps
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            bail!("pipeline requires pipeline string or steps array");
        };

        let output = crate::pipeline::run_output(&self.engine, &pipeline);
        let text = output.render();
        Ok(ToolOutput::new(text, output.to_json(&pipeline)))
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<McpMessage>> {
    let Some(first_line) = read_non_empty_line(reader)? else {
        return Ok(None);
    };

    let first_trimmed = trim_line_end(&first_line);
    if trim_ascii_start(first_trimmed).starts_with(b"{")
        || trim_ascii_start(first_trimmed).starts_with(b"[")
    {
        return Ok(Some(McpMessage {
            body: first_line,
            framing: StdioFraming::NewlineDelimited,
        }));
    }

    let mut content_length = parse_content_length_header(first_trimmed)?;
    loop {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = trim_line_end(&line);
        if trimmed.is_empty() {
            break;
        }
        if let Some(len) = parse_content_length_header(trimmed)? {
            content_length = Some(len);
        }
    }

    let len = content_length.context("missing Content-Length")?;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(McpMessage {
        body,
        framing: StdioFraming::ContentLength,
    }))
}

fn read_non_empty_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    loop {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(None);
        }
        if !trim_line_end(&line).is_empty() {
            return Ok(Some(line));
        }
    }
}

fn trim_line_end(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .or_else(|| line.strip_suffix(b"\r"))
        .unwrap_or(line)
}

fn trim_ascii_start(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(line.len());
    &line[start..]
}

fn trim_ascii(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &line[start..end]
}

fn parse_content_length_header(line: &[u8]) -> Result<Option<usize>> {
    let Some(colon_idx) = line.iter().position(|byte| *byte == b':') else {
        return Ok(None);
    };
    let (name, value) = line.split_at(colon_idx);
    if name.eq_ignore_ascii_case(b"content-length") {
        let value = trim_ascii(&value[1..]);
        let value = std::str::from_utf8(value).context("invalid Content-Length header")?;
        return Ok(Some(value.parse::<usize>()?));
    }
    Ok(None)
}

fn requested_protocol_version(params: Option<&Value>) -> &str {
    params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_MCP_PROTOCOL_VERSION)
}

fn write_response(writer: &mut impl Write, framing: StdioFraming, response: &Value) -> Result<()> {
    let body = serde_json::to_vec(response)?;
    match framing {
        StdioFraming::ContentLength => {
            write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
            writer.write_all(&body)?;
        }
        StdioFraming::NewlineDelimited => {
            writer.write_all(&body)?;
            writer.write_all(b"\n")?;
        }
    }
    writer.flush()?;
    Ok(())
}

fn tool_response(id: Value, result: Result<ToolOutput>, include_structured_content: bool) -> Value {
    match result {
        Ok(output) => {
            let mut result = json!({
                "content": [{ "type": "text", "text": output.text }],
                "isError": false
            });
            if include_structured_content && !output.structured.is_null() {
                result["structuredContent"] = output.structured;
            }
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
        }
        Err(err) => {
            let mut response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("error: {err}") }],
                    "isError": true
                }
            });
            if include_structured_content {
                response["result"]["structuredContent"] = json!({ "error": err.to_string() });
            }
            response
        }
    }
}

fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}

fn graph_status(path: &PathBuf) -> Value {
    match std::fs::metadata(path) {
        Ok(metadata) => json!({
            "path": path.display().to_string(),
            "exists": true,
            "size_bytes": metadata.len(),
            "size_mb": metadata.len() as f64 / (1024.0 * 1024.0),
        }),
        Err(_) => json!({
            "path": path.display().to_string(),
            "exists": false,
        }),
    }
}

fn tools() -> Value {
    json!([
        tool(
            "files",
            "List indexed files with optional path, glob, language, and line-count filters.",
            json!({"type":"object","properties":{"path":{"type":"string","description":"Optional project-relative path prefix."},"path_glob":{"type":"string"},"language":{"type":"string","description":"Language name such as typescript, rust, json, or markdown."},"min_lines":{"type":"integer"},"max_lines":{"type":"integer"},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."}},"required":[]})
        ),
        tool(
            "list",
            "List immediate children of a directory.",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":[]})
        ),
        tool(
            "glob",
            "Match indexed paths using a glob pattern.",
            json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]})
        ),
        tool(
            "path_search",
            "Fuzzy path search against indexed file names.",
            json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"},"max":{"type":"integer"}},"required":["query"]})
        ),
        tool(
            "outline",
            "Return symbols and imports for one file.",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})
        ),
        tool(
            "symbol_defs",
            "Find exact symbol definitions.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]})
        ),
        tool(
            "symbol_search",
            "Fuzzy symbol search for partial names such as createAgent matching createProjectAgent.",
            json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."}},"required":["query"]})
        ),
        tool(
            "word_refs",
            "Find exact identifier or word occurrences, including definitions and declarations.",
            json!({"type":"object","properties":{"word":{"type":"string"}},"required":["word"]})
        ),
        tool(
            "text_search",
            "Search indexed text by substring or regex. Supports scope, compact, paths_only, and path_glob.",
            json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"},"regex":{"type":"boolean"},"scope":{"type":"boolean"},"compact":{"type":"boolean"},"paths_only":{"type":"boolean"},"path_glob":{"type":"string"}},"required":["query"]})
        ),
        tool(
            "callers",
            "Find non-definition call sites/usages for a symbol; excludes declarations and type aliases.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]})
        ),
        tool(
            "brief",
            "Build a compact context bundle for an explicit code task. Best with symbol names, path fragments, or scoped keywords; not natural-language QA.",
            json!({"type":"object","properties":{"task":{"type":"string"},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."},"path_prefix":{"type":"string","description":"Restrict context to a project-relative path prefix."},"path":{"type":"string","description":"Alias for path_prefix."},"path_glob":{"type":"string"},"language":{"type":"string"}},"required":["task"]})
        ),
        tool(
            "trace_deps",
            "Trace resolved project-file imported_by or depends_on relationships. External packages are not returned as dependencies; depends_on includes unresolved local imports separately.",
            json!({"type":"object","properties":{"path":{"type":"string"},"direction":{"type":"string","enum":["imported_by","depends_on"]},"transitive":{"type":"boolean"}},"required":["path"]})
        ),
        tool(
            "read",
            "Read file contents with optional line range, compact mode, and if_hash.",
            json!({"type":"object","properties":{"path":{"type":"string"},"line_start":{"type":"integer"},"line_end":{"type":"integer"},"compact":{"type":"boolean"},"if_hash":{"type":"string"}},"required":["path"]})
        ),
        tool(
            "patch",
            "Apply line-based replace, insert, or delete with optional if_hash and dry_run.",
            json!({"type":"object","properties":{"path":{"type":"string"},"op":{"type":"string","enum":["replace","insert","delete"]},"content":{"type":"string"},"range_start":{"type":"integer"},"range_end":{"type":"integer"},"after":{"type":"integer"},"if_hash":{"type":"string"},"dry_run":{"type":"boolean"}},"required":["path","op"]})
        ),
        tool(
            "create",
            "Create a file safely. Refuses overwrites unless overwrite is true.",
            json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"overwrite":{"type":"boolean"},"dry_run":{"type":"boolean"}},"required":["path"]})
        ),
        tool(
            "changes",
            "List files changed since a sequence number.",
            json!({"type":"object","properties":{"since":{"type":"integer"}},"required":[]})
        ),
        tool(
            "recent",
            "List recently modified files.",
            json!({"type":"object","properties":{"limit":{"type":"integer"}},"required":[]})
        ),
        tool(
            "status",
            "Return index status.",
            json!({"type":"object","properties":{},"required":[]})
        ),
        tool(
            "reindex",
            "Rebuild the in-memory index from the MCP project root and persist the graph when persistence is enabled.",
            json!({"type":"object","properties":{},"required":[]})
        ),
        tool(
            "clear_index",
            "Clear the in-memory index and remove the graph file if present.",
            json!({"type":"object","properties":{},"required":[]})
        ),
        tool(
            "audit",
            "Run a review-oriented architecture audit over the indexed project. config is a TOML file path; max is an alias for max_results.",
            json!({"type":"object","properties":{"max_results":{"type":"integer"},"max":{"type":"integer"},"since":{"type":"string"},"config":{"type":"string","description":"Path to a Lexa audit TOML config file, such as lexa.toml or .lexa/audit.toml. This is not a named preset."},"no_config":{"type":"boolean"},"include":{"type":"array","items":{"type":"string","enum":["dead-code"]}}},"required":[]})
        ),
        tool(
            "pipeline",
            "Run an advanced composable pipeline. Prefer steps array form. Commands: glob/find, fuzzy/path_search, search/text_search, filter, outline, deps, read, sort, limit, count.",
            json!({"type":"object","properties":{"pipeline":{"type":"string","description":"Advanced pipe string, e.g. glob src/**/*.rs | search main | limit 5."},"steps":{"type":"array","items":{"type":"string"},"description":"Recommended form; each item is one pipeline step, e.g. [\"glob src/**/*.rs\", \"search main\", \"limit 5\"]. Put search terms inside the relevant step."}},"required":[]})
        )
    ])
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    opt_str(args, key).with_context(|| format!("missing required string: {key}"))
}

fn req_any_str<'a>(args: &'a Value, keys: &[&str]) -> Result<&'a str> {
    keys.iter()
        .find_map(|key| opt_str(args, key))
        .with_context(|| format!("missing required string: {}", keys.join("|")))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn opt_u32(args: &Value, key: &str) -> Option<u32> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

fn opt_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

fn opt_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
}

fn audit_includes(args: &Value) -> Result<audit::AuditIncludes> {
    let mut includes = audit::AuditIncludes::default();
    let Some(values) = args.get("include").and_then(Value::as_array) else {
        return Ok(includes);
    };

    for value in values {
        match value.as_str() {
            Some("dead-code") => includes.dead_code = true,
            Some(other) => bail!("unknown audit include: {other}"),
            None => bail!("audit include values must be strings"),
        }
    }

    Ok(includes)
}

fn parse_edit_op(op: &str) -> Result<EditOp> {
    match op {
        "replace" => Ok(EditOp::Replace),
        "insert" => Ok(EditOp::Insert),
        "delete" => Ok(EditOp::Delete),
        _ => bail!("op must be replace, insert, or delete"),
    }
}

fn store_op(op: EditOp) -> store::Op {
    match op {
        EditOp::Replace => store::Op::Replace,
        EditOp::Insert => store::Op::Insert,
        EditOp::Delete => store::Op::Delete,
    }
}

fn render_search_results(query: &str, results: &[crate::types::SearchResult]) -> String {
    if results.is_empty() {
        return format!("No results found for '{query}'");
    }
    let mut out = format!("{} results for '{query}':\n", results.len());
    for result in results {
        out.push_str(&format!(
            "{}:{}: {}\n",
            result.path, result.line_num, result.line_text
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_message_accepts_newline_delimited_json() {
        let mut reader = Cursor::new(br#"{"jsonrpc":"2.0","id":0,"method":"initialize"}"#.to_vec());
        let message = read_message(&mut reader).unwrap().unwrap();

        assert_eq!(message.framing, StdioFraming::NewlineDelimited);
        assert_eq!(
            serde_json::from_slice::<Value>(&message.body).unwrap(),
            json!({"jsonrpc":"2.0","id":0,"method":"initialize"})
        );
    }

    #[test]
    fn read_message_accepts_content_length_framing() {
        let body = br#"{"jsonrpc":"2.0","id":0,"method":"initialize"}"#;
        let framed = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut input = framed.into_bytes();
        input.extend_from_slice(body);
        let mut reader = Cursor::new(input);

        let message = read_message(&mut reader).unwrap().unwrap();

        assert_eq!(message.framing, StdioFraming::ContentLength);
        assert_eq!(message.body, body);
    }

    #[test]
    fn read_message_handles_non_utf8_input_without_io_utf8_error() {
        let mut reader = Cursor::new(vec![0xff, b'\n']);

        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn write_response_uses_newline_delimited_framing() {
        let mut output = Vec::new();
        let response = json!({"jsonrpc":"2.0","id":0,"result":{}});

        write_response(&mut output, StdioFraming::NewlineDelimited, &response).unwrap();

        let mut expected = serde_json::to_vec(&response).unwrap();
        expected.push(b'\n');

        assert_eq!(output, expected);
    }

    #[test]
    fn write_response_uses_content_length_framing() {
        let mut output = Vec::new();
        let response = json!({"jsonrpc":"2.0","id":0,"result":{}});

        write_response(&mut output, StdioFraming::ContentLength, &response).unwrap();

        let body = serde_json::to_vec(&response).unwrap();
        let mut expected = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        expected.extend_from_slice(&body);

        assert_eq!(output, expected);
    }

    #[test]
    fn tool_response_omits_structured_content_by_default() {
        let response = tool_response(
            json!(1),
            Ok(ToolOutput::new(
                "plain text".to_string(),
                json!({"count": 1}),
            )),
            false,
        );

        assert_eq!(
            response["result"]["content"][0]["text"],
            Value::String("plain text".to_string())
        );
        assert!(response["result"].get("structuredContent").is_none());
    }

    #[test]
    fn tool_response_includes_structured_content_when_enabled() {
        let response = tool_response(
            json!(1),
            Ok(ToolOutput::new(
                "plain text".to_string(),
                json!({"count": 1}),
            )),
            true,
        );

        assert_eq!(response["result"]["structuredContent"], json!({"count": 1}));
    }

    #[test]
    fn tool_error_response_omits_structured_content_by_default() {
        let response = tool_response(json!(1), Err(anyhow::anyhow!("bad input")), false);

        assert_eq!(response["result"]["isError"], Value::Bool(true));
        assert!(response["result"].get("structuredContent").is_none());
    }

    #[test]
    fn initialize_uses_requested_protocol_version() {
        let params = json!({ "protocolVersion": "2025-11-25" });

        assert_eq!(requested_protocol_version(Some(&params)), "2025-11-25");
    }

    #[test]
    fn tools_use_unprefixed_names() {
        let tools = tools();
        let names = tools
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(names.contains(&"files"));
        assert!(names.contains(&"path_search"));
        assert!(names.contains(&"symbol_defs"));
        assert!(names.contains(&"symbol_search"));
        assert!(names.contains(&"word_refs"));
        assert!(names.contains(&"text_search"));
        assert!(names.contains(&"callers"));
        assert!(names.contains(&"create"));
        assert!(names.contains(&"audit"));
        assert!(names.contains(&"reindex"));
        assert!(names.contains(&"clear_index"));
        assert!(names.iter().all(|name| !name.starts_with("lexa_")));
        assert!(!names.contains(&"lexa_map"));
        assert!(!names.contains(&"lexa_find_path"));
        assert!(!names.contains(&"lexa_find_symbol"));
        assert!(!names.contains(&"lexa_find_word"));
        assert!(!names.contains(&"lexa_search"));
        assert!(!names.contains(&"lexa_find_callers"));
    }

    #[test]
    fn outline_missing_file_reports_clean_error() {
        let root = tempfile::tempdir().unwrap();
        let server = McpServer::new(
            Engine::new(32),
            root.path().to_path_buf(),
            root.path().join(".lexa/graph.lexa"),
            false,
            false,
        );

        let err = match server.tool_outline("apps/desktop/CLAUDE.md") {
            Ok(_) => panic!("expected missing file error"),
            Err(err) => err.to_string(),
        };

        assert_eq!(err, "file not found: apps/desktop/CLAUDE.md");
        assert!(!err.contains(root.path().to_string_lossy().as_ref()));
        assert!(!err.contains("canonicalize"));
    }

    #[test]
    fn pipeline_steps_ignore_removed_query_argument() {
        let root = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(32);
        engine.index_file("src/a.ts", "export type AgentRunRequest = {};\n");
        let server = McpServer::new(
            engine,
            root.path().to_path_buf(),
            root.path().join(".lexa/graph.lexa"),
            false,
            false,
        );

        let output = server
            .tool_pipeline(&json!({
            "query": "ignored",
            "steps": ["search AgentRunRequest", "limit 3"]
            }))
            .unwrap();

        assert!(output.text.contains("AgentRunRequest"));
    }

    #[test]
    fn pipeline_query_only_is_not_supported() {
        let root = tempfile::tempdir().unwrap();
        let server = McpServer::new(
            Engine::new(32),
            root.path().to_path_buf(),
            root.path().join(".lexa/graph.lexa"),
            false,
            false,
        );

        let err = match server.tool_pipeline(&json!({"query": "search AgentRunRequest | limit 1"}))
        {
            Ok(_) => panic!("expected query-only error"),
            Err(err) => err.to_string(),
        };

        assert_eq!(err, "pipeline requires pipeline string or steps array");
    }

    #[test]
    fn pipeline_schema_omits_query_argument() {
        let tools = tools();
        let pipeline = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("pipeline"))
            .unwrap();
        let properties = pipeline["inputSchema"]["properties"].as_object().unwrap();

        assert!(properties.contains_key("pipeline"));
        assert!(properties.contains_key("steps"));
        assert!(!properties.contains_key("query"));
    }
}
