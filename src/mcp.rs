use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::edit::{self, EditOp};
use crate::engine::{Engine, SearchOptions};
use crate::snapshot;
use crate::store;

const DEFAULT_MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub struct McpServer {
    engine: Engine,
    root: PathBuf,
    graph_path: PathBuf,
    persist_graph: bool,
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
    pub fn new(engine: Engine, root: PathBuf, graph_path: PathBuf, persist_graph: bool) -> Self {
        Self {
            engine,
            root,
            graph_path,
            persist_graph,
        }
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
                Some(tool_response(id, result))
            }
            "ping" => id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": {} })),
            _ => id.map(|id| json_rpc_error(Some(id), -32601, "method not found")),
        }
    }

    fn call_tool(&mut self, name: &str, args: &Value) -> Result<ToolOutput> {
        match name {
            "lexa_map" => Ok(self.tool_map(args)),
            "lexa_list" => Ok(self.tool_list(opt_str(args, "path").unwrap_or(""))),
            "lexa_glob" => self.tool_glob(req_str(args, "pattern")?),
            "lexa_find_path" => self.tool_find_path(
                req_any_str(args, &["query", "path", "pattern", "name"])?,
                opt_usize(args, "max_results")
                    .or_else(|| opt_usize(args, "max"))
                    .unwrap_or(20),
            ),
            "lexa_outline" => self.tool_outline(req_str(args, "path")?),
            "lexa_find_symbol" => self.tool_find_symbol(req_any_str(args, &["name", "query"])?),
            "lexa_find_word" => self.tool_find_word(req_any_str(args, &["word", "query"])?),
            "lexa_search" => self.tool_search(args),
            "lexa_find_callers" => self.tool_find_callers(req_any_str(args, &["name", "query"])?),
            "lexa_brief" => self.tool_brief(req_any_str(args, &["task", "query"])?),
            "lexa_trace_deps" => self.tool_trace_deps(args),
            "lexa_read" => self.tool_read(args),
            "lexa_patch" => self.tool_patch(args),
            "lexa_changes" => Ok(self.tool_changes(opt_u64(args, "since").unwrap_or(0))),
            "lexa_recent" => Ok(self.tool_recent(opt_usize(args, "limit").unwrap_or(10))),
            "lexa_status" => Ok(self.tool_status()),
            "lexa_pipeline" => self.tool_pipeline(args),
            _ => bail!("unknown tool: {name}"),
        }
    }

    fn tool_map(&self, args: &Value) -> ToolOutput {
        let limit = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .unwrap_or(200);
        let mut files = self.engine.file_map();
        let total = files.len();
        let truncated = files.len() > limit;
        files.truncate(limit);
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
        let Some(outline) = self.engine.get_outline(path) else {
            bail!("file not found: {path}");
        };

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
        if !outline.symbols.is_empty() {
            out.push_str("\nSymbols:\n");
            for sym in &outline.symbols {
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

    fn tool_brief(&self, task: &str) -> Result<ToolOutput> {
        let max_results = 10;
        let text = self.engine.build_context(task, max_results);
        let details = self.engine.build_context_details(task, max_results);
        Ok(ToolOutput::new(text, json!(details)))
    }

    fn tool_trace_deps(&self, args: &Value) -> Result<ToolOutput> {
        let path = req_str(args, "path")?;
        let direction = opt_str(args, "direction").unwrap_or("imported_by");
        let transitive = opt_bool(args, "transitive").unwrap_or(false);
        let deps = match (direction, transitive) {
            ("depends_on", true) => self.engine.get_transitive_depends_on(path),
            ("depends_on", false) => self.engine.get_depends_on(path),
            ("imported_by", true) => self.engine.get_transitive_imported_by(path),
            ("imported_by", false) => self.engine.get_imported_by(path),
            _ => bail!("direction must be imported_by or depends_on"),
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
            }),
        ))
    }

    fn tool_read(&self, args: &Value) -> Result<ToolOutput> {
        let path = req_str(args, "path")?;
        ensure_safe_relative_path(path)?;
        let line_start = opt_u32(args, "line_start");
        let line_end = opt_u32(args, "line_end");
        let result = self
            .engine
            .read_file_rich(
                path,
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
        let rel_path = req_str(args, "path")?;
        ensure_safe_relative_path(rel_path)?;
        let abs_path = self.root.join(rel_path);
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
                .index_edited_file(rel_path, &result.new_content, store_op(op));
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
        let text = format!(
            "files: {}\nseq: {} (session-local)\ngraph: {}\nchange_history_persisted: false",
            self.engine.file_count(),
            self.engine.store().current_seq(),
            self.graph_path.display()
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
                "graph": self.graph_path.display().to_string(),
            }),
        )
    }

    fn tool_pipeline(&self, args: &Value) -> Result<ToolOutput> {
        let pipeline =
            if let Some(text) = opt_str(args, "query").or_else(|| opt_str(args, "pipeline")) {
                text.to_string()
            } else if let Some(steps) = args.get("steps").and_then(Value::as_array) {
                steps
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" | ")
            } else {
                bail!("pipeline requires query/pipeline string or steps array");
            };

        let text = crate::pipeline::run(&self.engine, &pipeline);
        Ok(ToolOutput::new(
            text.clone(),
            json!({
                "pipeline": pipeline,
                "text": text,
                "structured": false,
                "note": "Pipeline currently returns text because each stage can change result shape."
            }),
        ))
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<McpMessage>> {
    let Some(first_line) = read_non_empty_line(reader)? else {
        return Ok(None);
    };

    let first_trimmed = trim_line_end(&first_line);
    if first_trimmed.trim_start().starts_with(['{', '[']) {
        return Ok(Some(McpMessage {
            body: first_line.into_bytes(),
            framing: StdioFraming::NewlineDelimited,
        }));
    }

    let mut content_length = parse_content_length_header(first_trimmed)?;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
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

fn read_non_empty_line(reader: &mut impl BufRead) -> Result<Option<String>> {
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Ok(None);
        }
        if !trim_line_end(&line).is_empty() {
            return Ok(Some(line));
        }
    }
}

fn trim_line_end(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn parse_content_length_header(line: &str) -> Result<Option<usize>> {
    let Some((name, value)) = line.split_once(':') else {
        return Ok(None);
    };
    if name.eq_ignore_ascii_case("content-length") {
        return Ok(Some(value.trim().parse::<usize>()?));
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

fn tool_response(id: Value, result: Result<ToolOutput>) -> Value {
    match result {
        Ok(output) => {
            let mut result = json!({
                "content": [{ "type": "text", "text": output.text }],
                "isError": false
            });
            if !output.structured.is_null() {
                result["structuredContent"] = output.structured;
            }
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
        }
        Err(err) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": format!("error: {err}") }],
                "structuredContent": { "error": err.to_string() },
                "isError": true
            }
        }),
    }
}

fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}

fn tools() -> Value {
    json!([
        tool(
            "lexa_map",
            "Whole-repo file map with language, line, and symbol counts.",
            json!({"type":"object","properties":{"max_results":{"type":"integer"},"max":{"type":"integer"}},"required":[]})
        ),
        tool(
            "lexa_list",
            "List immediate children of a directory.",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":[]})
        ),
        tool(
            "lexa_glob",
            "Match indexed paths using a glob pattern.",
            json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]})
        ),
        tool(
            "lexa_find_path",
            "Fuzzy path search against indexed file names.",
            json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"},"max":{"type":"integer"}},"required":["query"]})
        ),
        tool(
            "lexa_outline",
            "Return symbols and imports for one file.",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})
        ),
        tool(
            "lexa_find_symbol",
            "Find definitions of a symbol by exact name.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]})
        ),
        tool(
            "lexa_find_word",
            "Find exact word or identifier occurrences.",
            json!({"type":"object","properties":{"word":{"type":"string"}},"required":["word"]})
        ),
        tool(
            "lexa_search",
            "Search indexed text. Supports regex, scope, compact, paths_only, and path_glob.",
            json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"},"regex":{"type":"boolean"},"scope":{"type":"boolean"},"compact":{"type":"boolean"},"paths_only":{"type":"boolean"},"path_glob":{"type":"string"}},"required":["query"]})
        ),
        tool(
            "lexa_find_callers",
            "Find non-definition call sites for a symbol.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]})
        ),
        tool(
            "lexa_brief",
            "Compose task-focused context from symbols and search snippets.",
            json!({"type":"object","properties":{"task":{"type":"string"}},"required":["task"]})
        ),
        tool(
            "lexa_trace_deps",
            "Trace imported_by or depends_on relationships.",
            json!({"type":"object","properties":{"path":{"type":"string"},"direction":{"type":"string","enum":["imported_by","depends_on"]},"transitive":{"type":"boolean"}},"required":["path"]})
        ),
        tool(
            "lexa_read",
            "Read file contents with optional line range, compact mode, and if_hash.",
            json!({"type":"object","properties":{"path":{"type":"string"},"line_start":{"type":"integer"},"line_end":{"type":"integer"},"compact":{"type":"boolean"},"if_hash":{"type":"string"}},"required":["path"]})
        ),
        tool(
            "lexa_patch",
            "Apply line-based replace, insert, or delete with optional if_hash and dry_run.",
            json!({"type":"object","properties":{"path":{"type":"string"},"op":{"type":"string","enum":["replace","insert","delete"]},"content":{"type":"string"},"range_start":{"type":"integer"},"range_end":{"type":"integer"},"after":{"type":"integer"},"if_hash":{"type":"string"},"dry_run":{"type":"boolean"}},"required":["path","op"]})
        ),
        tool(
            "lexa_changes",
            "List files changed since a sequence number.",
            json!({"type":"object","properties":{"since":{"type":"integer"}},"required":[]})
        ),
        tool(
            "lexa_recent",
            "List recently modified files.",
            json!({"type":"object","properties":{"limit":{"type":"integer"}},"required":[]})
        ),
        tool(
            "lexa_status",
            "Return index status.",
            json!({"type":"object","properties":{},"required":[]})
        ),
        tool(
            "lexa_pipeline",
            "Run a composable pipeline string such as 'glob src/**/*.rs | search main | limit 5'.",
            json!({"type":"object","properties":{"pipeline":{"type":"string"},"query":{"type":"string"},"steps":{"type":"array","items":{"type":"string"}}},"required":[]})
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

fn ensure_safe_relative_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("path must be relative to project root and must not contain ..");
    }
    Ok(())
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

fn rich_results_json(results: &[crate::engine::RichSearchResult]) -> Vec<Value> {
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
    fn initialize_uses_requested_protocol_version() {
        let params = json!({ "protocolVersion": "2025-11-25" });

        assert_eq!(requested_protocol_version(Some(&params)), "2025-11-25");
    }
}
