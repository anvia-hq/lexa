use crate::engine::{RichSearchResult, WordSearchResult};
use crate::types::{SearchResult, SymbolKind};
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn rich_results_json(results: &[RichSearchResult]) -> Vec<Value> {
    results
        .iter()
        .map(|result| {
            json!({
                "path": &result.path,
                "line": result.line_num,
                "text": &result.line_text,
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

pub fn agent_toon(tool: &str, payload: Value) -> Result<String> {
    let value = agent_result_value(tool, payload);
    toon_format::encode_default(&value).context("failed to encode agent result as TOON")
}

pub fn agent_error_toon(tool: &str, error: impl AsRef<str>) -> Result<String> {
    let value = json!({
        "tool": canonical_tool(tool),
        "ok": false,
        "error": error.as_ref(),
    });
    toon_format::encode_default(&value).context("failed to encode agent error as TOON")
}

pub fn search_result_path_facets(results: &[SearchResult]) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for result in results {
        let prefix = result
            .path
            .split('/')
            .next()
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or(".");
        *counts.entry(prefix.to_string()).or_default() += 1;
    }

    Value::Array(
        counts
            .into_iter()
            .map(|(path_prefix, count)| json!({ "path_prefix": path_prefix, "count": count }))
            .collect(),
    )
}

pub fn word_result_path_facets(results: &[WordSearchResult]) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for result in results {
        let prefix = result
            .path
            .split('/')
            .next()
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or(".");
        *counts.entry(prefix.to_string()).or_default() += 1;
    }

    Value::Array(
        counts
            .into_iter()
            .map(|(path_prefix, count)| json!({ "path_prefix": path_prefix, "count": count }))
            .collect(),
    )
}

pub fn word_result_kind_facets(results: &[WordSearchResult]) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for result in results {
        *counts.entry(result.kind.clone()).or_default() += 1;
    }

    Value::Array(
        counts
            .into_iter()
            .map(|(kind, count)| json!({ "kind": kind, "count": count }))
            .collect(),
    )
}

pub fn agent_result_value(tool: &str, payload: Value) -> Value {
    let tool = canonical_tool(tool);
    let mut result = if let Some(error) = payload.get("error").and_then(Value::as_str) {
        let mut map = Map::new();
        map.insert("tool".to_string(), s(tool));
        map.insert("ok".to_string(), Value::Bool(false));
        map.insert("error".to_string(), s(error));
        flatten_metadata(&mut map, without_keys(&payload, &["error"]));
        Value::Object(map)
    } else {
        match tool.as_str() {
            "files" => files_result(&tool, payload),
            "list" => list_result(&tool, payload),
            "glob" => glob_result(&tool, payload),
            "path_search" => path_search_result(&tool, payload),
            "outline" => outline_result(&tool, payload),
            "symbol_defs" => symbol_defs_result(&tool, payload),
            "symbol_search" => symbol_search_result(&tool, payload),
            "word_refs" => word_refs_result(&tool, payload),
            "text_search" => search_result(&tool, payload),
            "callers" => callers_result(&tool, payload),
            "brief" => brief_result(&tool, payload),
            "trace_deps" => trace_deps_result(&tool, payload),
            "read" => read_result(&tool, payload),
            "patch" | "create" => edit_result(&tool, payload),
            "changes" => changes_result(&tool, payload),
            "recent" => recent_result(&tool, payload),
            "status" | "index" | "reindex" | "clear_index" => summary_only_result(&tool, payload),
            "audit" => audit_result(&tool, payload),
            "pipeline" => pipeline_result(&tool, payload),
            _ => Value::Object(base(&tool, payload)),
        }
    };
    prune_empty_and_null(&mut result);
    result
}

fn canonical_tool(tool: &str) -> String {
    tool.replace('-', "_")
}

fn base(tool: &str, summary: Value) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("tool".to_string(), Value::String(tool.to_string()));
    flatten_metadata(&mut map, summary);
    map
}

fn flatten_metadata(map: &mut Map<String, Value>, metadata: Value) {
    let Value::Object(obj) = metadata else {
        insert_if_kept(map, "value", metadata);
        return;
    };
    for (key, value) in obj {
        insert_if_kept(map, &key, value);
    }
}

fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        if keep_value(&value) {
            map.insert(key.to_string(), value);
        }
    }
    Value::Object(map)
}

fn array(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(items.into_iter().collect())
}

fn row(items: impl IntoIterator<Item = Value>) -> Value {
    Value::Array(items.into_iter().collect())
}

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}

fn n(value: usize) -> Value {
    json!(value)
}

fn cols(names: &[&str]) -> Value {
    array(names.iter().map(|name| s(*name)))
}

#[derive(Clone, Copy)]
enum PathTarget<'a> {
    Rows {
        cols_key: &'a str,
        rows_key: &'a str,
        col: &'a str,
    },
    Array {
        key: &'a str,
    },
}

fn files_result(tool: &str, payload: Value) -> Value {
    let summary = drop_false_defaults(pick(
        &payload,
        &["count", "total", "limit", "truncated", "filters"],
    ));
    let files = payload
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let language_filter = payload
        .get("filters")
        .and_then(|filters| filters.get("language"))
        .and_then(Value::as_str);
    let omit_language = language_filter.is_some()
        && files.iter().all(|file| {
            file.get("language")
                .and_then(Value::as_str)
                .is_some_and(|language| Some(language) == language_filter)
        });
    let rows = files
        .into_iter()
        .map(|file| {
            if omit_language {
                row([
                    get(file, "path"),
                    get(file, "line_count"),
                    get(file, "symbol_count"),
                ])
            } else {
                file_row(file)
            }
        })
        .collect::<Vec<_>>();
    let columns = if omit_language {
        &["path", "lines", "symbols"][..]
    } else {
        &["path", "lang", "lines", "symbols"][..]
    };
    let mut map = with_rows_map(tool, summary, columns, rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn list_result(tool: &str, payload: Value) -> Value {
    let summary = pick(&payload, &["path", "count"]);
    let rows = payload
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|entry| row([get(entry, "name"), get(entry, "kind")]))
        .collect::<Vec<_>>();
    with_rows(tool, summary, &["name", "kind"], rows)
}

fn glob_result(tool: &str, payload: Value) -> Value {
    let summary = drop_false_defaults(pick(
        &payload,
        &["pattern", "count", "total", "limit", "truncated"],
    ));
    let paths = payload
        .get("paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut map = base(tool, summary);
    insert_if_kept(&mut map, "paths", array(paths.into_iter().cloned()));
    apply_path_compression(&mut map, &[PathTarget::Array { key: "paths" }]);
    Value::Object(map)
}

fn path_search_result(tool: &str, payload: Value) -> Value {
    let summary = pick(&payload, &["query", "count", "limit"]);
    let rows = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|result| row([get(result, "path"), get(result, "score")]))
        .collect::<Vec<_>>();
    let mut map = with_rows_map(tool, summary, &["path", "score"], rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn outline_result(tool: &str, payload: Value) -> Value {
    let symbols = payload
        .get("symbols")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|symbol| {
            !matches!(
                symbol.get("kind").and_then(Value::as_str),
                Some("Import" | "import")
            )
        })
        .collect::<Vec<_>>();
    let summary = object([
        ("path", get(&payload, "path")),
        ("lang", get(&payload, "language")),
        ("lines", get(&payload, "line_count")),
        ("symbols", n(symbols.len())),
        (
            "imports",
            n(payload
                .get("imports")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)),
        ),
    ]);
    let rows = symbols
        .into_iter()
        .map(|symbol| {
            row([
                get(symbol, "line_start"),
                get(symbol, "line_end"),
                kind_value(symbol),
                get(symbol, "name"),
                get(symbol, "detail"),
            ])
        })
        .collect::<Vec<_>>();
    let mut map = base(tool, summary);
    insert_if_kept(
        &mut map,
        "imports",
        payload.get("imports").cloned().unwrap_or(Value::Null),
    );
    insert_if_kept(
        &mut map,
        "unresolved_imports",
        payload
            .get("unresolved_imports")
            .cloned()
            .unwrap_or(Value::Null),
    );
    insert_rows(
        &mut map,
        &["line_start", "line_end", "kind", "name", "detail"],
        rows,
    );
    Value::Object(map)
}

fn symbol_defs_result(tool: &str, payload: Value) -> Value {
    let summary = pick(&payload, &["name", "count"]);
    let rows = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|result| {
            let symbol = result.get("symbol").unwrap_or(result);
            row([
                get(result, "path"),
                get(symbol, "line_start"),
                get(symbol, "line_end"),
                kind_value(symbol),
                get(symbol, "name"),
                get(symbol, "detail"),
            ])
        })
        .collect::<Vec<_>>();
    let mut map = base(tool, summary);
    insert_rows(
        &mut map,
        &["path", "line_start", "line_end", "kind", "name", "detail"],
        rows,
    );
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn symbol_search_result(tool: &str, payload: Value) -> Value {
    let summary = pick(&payload, &["query", "count", "limit"]);
    let rows = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|result| {
            row([
                get(result, "score"),
                get(result, "path"),
                get(result, "line_start"),
                get(result, "line_end"),
                kind_value(result),
                get(result, "name"),
                get(result, "detail"),
            ])
        })
        .collect::<Vec<_>>();
    let mut map = with_rows_map(
        tool,
        summary,
        &[
            "score",
            "path",
            "line_start",
            "line_end",
            "kind",
            "name",
            "detail",
        ],
        rows,
    );
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn word_refs_result(tool: &str, payload: Value) -> Value {
    let summary = pick(
        &payload,
        &[
            "word",
            "query",
            "count",
            "total",
            "limit",
            "next_cursor",
            "filters",
        ],
    );
    let rows = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(word_ref_row)
        .collect::<Vec<_>>();
    let mut map = base(tool, summary);
    insert_rows(&mut map, &["kind", "path", "line", "text"], rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    if let Some(cursor) = payload.get("next_cursor") {
        let word = payload
            .get("word")
            .or_else(|| payload.get("query"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut args = Map::new();
        insert_if_kept(&mut args, "word", s(word));
        insert_if_kept(&mut args, "cursor", cursor.clone());
        insert_if_kept(&mut args, "max_results", get(&payload, "limit"));
        if let Some(filters) = payload.get("filters").and_then(Value::as_object) {
            insert_if_kept(
                &mut args,
                "path_prefix",
                filters.get("path_prefix").cloned().unwrap_or(Value::Null),
            );
            insert_if_kept(
                &mut args,
                "path_glob",
                filters.get("path_glob").cloned().unwrap_or(Value::Null),
            );
        }
        insert_next_steps(
            &mut map,
            [NextStep::new(
                "word_refs",
                Value::Object(args),
                "continue paginated results",
            )],
        );
    }
    Value::Object(map)
}

fn search_result(tool: &str, payload: Value) -> Value {
    let summary = drop_false_defaults(pick(
        &payload,
        &[
            "query",
            "count",
            "limit",
            "truncated",
            "regex",
            "scope",
            "compact",
            "paths_only",
            "path_glob",
        ],
    ));
    let include_scope = payload
        .get("scope")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || payload
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|result| result.get("scope").is_some_and(keep_value));
    let rows = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|result| {
            if include_scope {
                let scope = result.get("scope").and_then(Value::as_object).map(|scope| {
                    let kind = scope
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let name = scope
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if kind.is_empty() && name.is_empty() {
                        String::new()
                    } else {
                        format!("{kind} {name}").trim().to_string()
                    }
                });
                row([
                    get(result, "path"),
                    line_value(result),
                    scope.map_or(Value::Null, s),
                    text_value(result),
                ])
            } else {
                search_row(result)
            }
        })
        .collect::<Vec<_>>();
    let columns = if include_scope {
        &["path", "line", "scope", "text"][..]
    } else {
        &["path", "line", "text"][..]
    };
    let mut map = with_rows_map(tool, summary, columns, rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn callers_result(tool: &str, payload: Value) -> Value {
    let summary = pick(&payload, &["name", "count", "limit"]);
    let rows = payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(search_row)
        .collect::<Vec<_>>();
    let mut map = with_rows_map(tool, summary, &["path", "line", "text"], rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn brief_result(tool: &str, payload: Value) -> Value {
    let mut summary = pick(
        &payload,
        &["task", "keywords", "max_results", "confidence", "note"],
    );
    trim_summary_keywords(&mut summary, 5);
    let mut map = base(tool, summary);
    let symbol_rows = payload
        .get("relevant_symbols")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|symbol| {
            row([
                get(symbol, "path"),
                get(symbol, "line_start"),
                get(symbol, "line_end"),
                get(symbol, "kind"),
                get(symbol, "name"),
            ])
        })
        .collect::<Vec<_>>();
    if !symbol_rows.is_empty() {
        map.insert(
            "symbol_cols".to_string(),
            cols(&["path", "line_start", "line_end", "kind", "name"]),
        );
        map.insert("symbols".to_string(), array(symbol_rows));
    }
    let snippet_rows = payload
        .get("snippets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(search_row)
        .collect::<Vec<_>>();
    if !snippet_rows.is_empty() {
        map.insert("snippet_cols".to_string(), cols(&["path", "line", "text"]));
        map.insert("snippets".to_string(), array(snippet_rows));
    }
    apply_path_compression(
        &mut map,
        &[
            PathTarget::Rows {
                cols_key: "symbol_cols",
                rows_key: "symbols",
                col: "path",
            },
            PathTarget::Rows {
                cols_key: "snippet_cols",
                rows_key: "snippets",
                col: "path",
            },
        ],
    );
    if should_emit_brief_next(
        &payload,
        map.get("symbols").is_none(),
        map.get("snippets").is_none(),
    ) {
        insert_next_steps(&mut map, brief_next_steps(&payload));
    }
    Value::Object(map)
}

fn trace_deps_result(tool: &str, payload: Value) -> Value {
    let summary = drop_false_defaults(pick(
        &payload,
        &["path", "direction", "transitive", "count"],
    ));
    let deps = payload
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut map = base(tool, summary);
    insert_if_kept(&mut map, "deps", array(deps.into_iter().cloned()));
    insert_if_kept(
        &mut map,
        "unresolved_imports",
        payload
            .get("unresolved_imports")
            .cloned()
            .unwrap_or(Value::Null),
    );
    apply_path_compression(&mut map, &[PathTarget::Array { key: "deps" }]);
    Value::Object(map)
}

fn read_result(tool: &str, payload: Value) -> Value {
    let summary = pick(
        &payload,
        &[
            "path",
            "hash",
            "unchanged",
            "line_start",
            "line_end",
            "compact",
        ],
    );
    let mut map = base(tool, summary);
    insert_if_kept(&mut map, "content", get(&payload, "content"));
    Value::Object(map)
}

fn edit_result(tool: &str, payload: Value) -> Value {
    let mut map = base(
        tool,
        without_keys(
            &payload,
            &["preview", "old_hash", "new_hash", "graph", "persisted"],
        ),
    );
    if let Some(preview) = payload.get("preview") {
        map.insert("content".to_string(), preview.clone());
    }
    Value::Object(map)
}

fn changes_result(tool: &str, payload: Value) -> Value {
    let summary = pick(
        &payload,
        &["since", "count", "change_history_persisted", "note"],
    );
    let rows = payload
        .get("changes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|change| row([get(change, "seq"), get(change, "path"), get(change, "op")]))
        .collect::<Vec<_>>();
    let mut map = with_rows_map(tool, summary, &["seq", "path", "op"], rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn recent_result(tool: &str, payload: Value) -> Value {
    let summary = pick(&payload, &["count", "limit"]);
    let rows = payload
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(file_row)
        .collect::<Vec<_>>();
    let mut map = with_rows_map(tool, summary, &["path", "lang", "lines", "symbols"], rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn summary_only_result(tool: &str, payload: Value) -> Value {
    Value::Object(base(tool, without_keys(&payload, &["refresh"])))
}

fn audit_result(tool: &str, payload: Value) -> Value {
    let mut summary = Map::new();
    insert_if_kept(&mut summary, "verdict", get(&payload, "verdict"));
    if let Some(obj) = payload.get("summary").and_then(Value::as_object) {
        for (key, value) in obj {
            insert_if_kept(&mut summary, key, value.clone());
        }
    }
    let all_rows = deduped_audit_rows(&payload);
    let total_rows = all_rows.len();
    let rows = all_rows.into_iter().take(12).collect::<Vec<_>>();
    let mut map = base(tool, Value::Object(summary));
    if total_rows > rows.len() {
        map.insert("shown_findings".to_string(), n(rows.len()));
    }
    insert_audit_rule_rows(&mut map, audit_rule_groups(&payload));
    insert_rows(
        &mut map,
        &["severity", "rule", "path", "line", "title", "instances"],
        rows,
    );
    apply_path_compression(
        &mut map,
        &[
            PathTarget::Rows {
                cols_key: "rule_cols",
                rows_key: "rules",
                col: "top_path",
            },
            PathTarget::Rows {
                cols_key: "cols",
                rows_key: "rows",
                col: "path",
            },
        ],
    );
    insert_next_steps(&mut map, audit_grouped_next_steps(&payload));
    Value::Object(map)
}

fn pipeline_result(tool: &str, payload: Value) -> Value {
    let summary = pick(
        &payload,
        &["pipeline", "result_type", "count", "kind", "message"],
    );
    let result_type = payload
        .get("result_type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let rows = payload
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| match result_type {
            "results" => search_row(item),
            "files" => row([item.clone()]),
            "outlines" => row([
                get(item, "path"),
                json!(item
                    .get("symbols")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)),
            ]),
            "deps" => row([
                get(item, "path"),
                json!(item
                    .get("depends_on")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)),
                json!(item
                    .get("imported_by")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)),
            ]),
            "reads" => row([get(item, "path"), get(item, "content")]),
            _ => row([item.clone()]),
        })
        .collect::<Vec<_>>();
    let columns = match result_type {
        "results" => &["path", "line", "text"][..],
        "files" => &["path"][..],
        "outlines" => &["path", "symbols"][..],
        "deps" => &["path", "depends_on", "imported_by"][..],
        "reads" => &["path", "content"][..],
        _ => &["value"][..],
    };
    let mut map = with_rows_map(tool, summary, columns, rows);
    apply_path_compression(
        &mut map,
        &[PathTarget::Rows {
            cols_key: "cols",
            rows_key: "rows",
            col: "path",
        }],
    );
    Value::Object(map)
}

fn with_rows(tool: &str, summary: Value, columns: &[&str], rows: Vec<Value>) -> Value {
    Value::Object(with_rows_map(tool, summary, columns, rows))
}

fn with_rows_map(
    tool: &str,
    summary: Value,
    columns: &[&str],
    rows: Vec<Value>,
) -> Map<String, Value> {
    let mut map = base(tool, summary);
    insert_rows(&mut map, columns, rows);
    map
}

fn insert_rows(map: &mut Map<String, Value>, columns: &[&str], rows: Vec<Value>) {
    if rows.is_empty() {
        return;
    }
    map.insert("cols".to_string(), cols(columns));
    map.insert("rows".to_string(), array(rows));
}

fn apply_path_compression(map: &mut Map<String, Value>, targets: &[PathTarget<'_>]) {
    if map.contains_key("root") {
        return;
    }

    let mut paths = Vec::new();
    for target in targets {
        collect_target_paths(map, *target, &mut paths);
    }
    let Some(root) = common_path_root(&paths) else {
        return;
    };
    if !path_root_saves_tokens(&root, &paths) {
        return;
    }

    for target in targets {
        strip_target_paths(map, *target, &root);
    }
    map.insert("root".to_string(), s(root));
}

fn collect_target_paths(map: &Map<String, Value>, target: PathTarget<'_>, out: &mut Vec<String>) {
    match target {
        PathTarget::Rows {
            cols_key,
            rows_key,
            col,
        } => {
            let Some(index) = column_index(map, cols_key, col) else {
                return;
            };
            for row in map
                .get(rows_key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(path) = row
                    .as_array()
                    .and_then(|values| values.get(index))
                    .and_then(Value::as_str)
                    .filter(|path| path_is_compressible(path))
                {
                    out.push(path.to_string());
                }
            }
        }
        PathTarget::Array { key } => {
            for item in map.get(key).and_then(Value::as_array).into_iter().flatten() {
                if let Some(path) = item.as_str().filter(|path| path_is_compressible(path)) {
                    out.push(path.to_string());
                }
            }
        }
    }
}

fn strip_target_paths(map: &mut Map<String, Value>, target: PathTarget<'_>, root: &str) {
    match target {
        PathTarget::Rows {
            cols_key,
            rows_key,
            col,
        } => {
            let Some(index) = column_index(map, cols_key, col) else {
                return;
            };
            for row in map
                .get_mut(rows_key)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let Some(values) = row.as_array_mut() else {
                    continue;
                };
                let Some(value) = values.get_mut(index) else {
                    continue;
                };
                if let Some(path) = value.as_str().and_then(|path| path.strip_prefix(root)) {
                    *value = s(path);
                }
            }
        }
        PathTarget::Array { key } => {
            for item in map
                .get_mut(key)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(path) = item.as_str().and_then(|path| path.strip_prefix(root)) {
                    *item = s(path);
                }
            }
        }
    }
}

fn column_index(map: &Map<String, Value>, cols_key: &str, col: &str) -> Option<usize> {
    map.get(cols_key)
        .and_then(Value::as_array)?
        .iter()
        .position(|value| value.as_str() == Some(col))
}

fn common_path_root(paths: &[String]) -> Option<String> {
    if paths.len() < 2 {
        return None;
    }
    let mut prefix = paths.first()?.as_str();
    for path in &paths[1..] {
        let common_len = prefix
            .char_indices()
            .zip(path.char_indices())
            .take_while(|((_, left), (_, right))| left == right)
            .last()
            .map(|((index, ch), _)| index + ch.len_utf8())
            .unwrap_or(0);
        prefix = &prefix[..common_len];
        if prefix.is_empty() {
            return None;
        }
    }
    let slash = prefix.rfind('/')?;
    let root = &prefix[..=slash];
    if root.is_empty() || paths.iter().any(|path| path == root) {
        None
    } else {
        Some(root.to_string())
    }
}

fn path_root_saves_tokens(root: &str, paths: &[String]) -> bool {
    root.len().saturating_mul(paths.len()) > root.len().saturating_add(6)
}

fn path_is_compressible(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains("://") && path.contains('/')
}

fn file_row(file: &Value) -> Value {
    row([
        get(file, "path"),
        get(file, "language"),
        get(file, "line_count"),
        get(file, "symbol_count"),
    ])
}

fn search_row(result: &Value) -> Value {
    row([get(result, "path"), line_value(result), text_value(result)])
}

fn word_ref_row(result: &Value) -> Value {
    row([
        get(result, "kind"),
        get(result, "path"),
        line_value(result),
        text_value(result),
    ])
}

fn line_value(value: &Value) -> Value {
    value
        .get("line")
        .or_else(|| value.get("line_num"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn text_value(value: &Value) -> Value {
    let raw = value
        .get("text")
        .or_else(|| value.get("line_text"))
        .cloned()
        .unwrap_or(Value::Null);
    let Some(text) = raw.as_str() else {
        return raw;
    };
    s(compact_text(text, 120))
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let compacted = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compacted.chars().count() <= max_chars {
        return compacted;
    }
    let keep = max_chars.saturating_sub(3);
    format!("{}...", compacted.chars().take(keep).collect::<String>())
}

fn kind_value(value: &Value) -> Value {
    let Some(kind) = value.get("kind").and_then(Value::as_str) else {
        return Value::Null;
    };
    if let Some(canonical) = canonical_symbol_kind(kind) {
        s(canonical)
    } else {
        s(kind.to_ascii_lowercase())
    }
}

fn canonical_symbol_kind(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Function" => SymbolKind::Function.as_str(),
        "StructDef" => SymbolKind::StructDef.as_str(),
        "EnumDef" => SymbolKind::EnumDef.as_str(),
        "UnionDef" => SymbolKind::UnionDef.as_str(),
        "Constant" => SymbolKind::Constant.as_str(),
        "Variable" => SymbolKind::Variable.as_str(),
        "Import" => SymbolKind::Import.as_str(),
        "TestDecl" => SymbolKind::TestDecl.as_str(),
        "CommentBlock" => SymbolKind::CommentBlock.as_str(),
        "TraitDef" => SymbolKind::TraitDef.as_str(),
        "ImplBlock" => SymbolKind::ImplBlock.as_str(),
        "TypeAlias" => SymbolKind::TypeAlias.as_str(),
        "MacroDef" => SymbolKind::MacroDef.as_str(),
        "Method" => SymbolKind::Method.as_str(),
        "ClassDef" => SymbolKind::ClassDef.as_str(),
        "InterfaceDef" => SymbolKind::InterfaceDef.as_str(),
        "Module" => SymbolKind::Module.as_str(),
        _ => return None,
    })
}

#[derive(Clone)]
struct NextStep {
    tool: String,
    args: Value,
}

impl NextStep {
    fn new(tool: impl Into<String>, args: Value, _reason: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args,
        }
    }
}

fn insert_next_steps(map: &mut Map<String, Value>, steps: impl IntoIterator<Item = NextStep>) {
    let mut seen = BTreeSet::new();
    let rows = steps
        .into_iter()
        .filter_map(|step| {
            if step.tool.is_empty() {
                return None;
            }
            let args = if step.args.is_null() {
                "{}".to_string()
            } else {
                step.args.to_string()
            };
            if !seen.insert((step.tool.clone(), args.clone())) {
                return None;
            }
            Some(row([s(step.tool), step.args]))
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return;
    }
    map.insert("next_cols".to_string(), cols(&["tool", "args"]));
    map.insert("next".to_string(), array(rows));
}

fn trim_summary_keywords(summary: &mut Value, limit: usize) {
    let Some(summary) = summary.as_object_mut() else {
        return;
    };
    let Some(keywords) = summary.get_mut("keywords").and_then(Value::as_array_mut) else {
        return;
    };
    let keyword_count = keywords.len();
    if keyword_count <= limit {
        return;
    }
    keywords.truncate(limit);
    summary.insert("keyword_count".to_string(), n(keyword_count));
}

fn brief_next_steps(payload: &Value) -> Vec<NextStep> {
    let task = payload
        .get("task")
        .or_else(|| payload.get("query"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    payload
        .get("suggested_next_steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let reason = step.as_str()?.to_string();
            let lower = reason.to_ascii_lowercase();
            if lower.contains("symbol-search") {
                Some(NextStep::new(
                    "symbol_search",
                    json!({ "query": task }),
                    reason,
                ))
            } else if lower.contains("text-search") {
                Some(NextStep::new(
                    "text_search",
                    json!({ "query": task }),
                    reason,
                ))
            } else {
                Some(NextStep::new("brief", json!({ "task": task }), reason))
            }
        })
        .collect()
}

fn should_emit_brief_next(payload: &Value, no_symbols: bool, no_snippets: bool) -> bool {
    if no_symbols && no_snippets {
        return true;
    }
    if payload
        .get("confidence")
        .and_then(Value::as_str)
        .is_some_and(|confidence| confidence.eq_ignore_ascii_case("low"))
    {
        return true;
    }
    payload
        .get("note")
        .and_then(Value::as_str)
        .is_some_and(|note| {
            let note = note.to_ascii_lowercase();
            note.contains("low-confidence") || note.contains("scope")
        })
}

#[derive(Clone)]
struct AuditRuleGroup {
    rule: String,
    severity: String,
    count: usize,
    top_path: String,
}

fn insert_audit_rule_rows(map: &mut Map<String, Value>, groups: Vec<AuditRuleGroup>) {
    let rows = groups
        .into_iter()
        .map(|group| {
            row([
                s(group.rule),
                s(group.severity),
                n(group.count),
                s(group.top_path),
            ])
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }
    map.insert(
        "rule_cols".to_string(),
        cols(&["rule", "severity", "count", "top_path"]),
    );
    map.insert("rules".to_string(), array(rows));
}

fn audit_rule_groups(payload: &Value) -> Vec<AuditRuleGroup> {
    let mut indexes = BTreeMap::<String, usize>::new();
    let mut groups = Vec::<AuditRuleGroup>::new();

    for finding in payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let rule = finding
            .get("rule")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let severity = finding
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("warning")
            .to_string();
        let path = finding
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let key = format!("{rule}\u{1f}{severity}");

        if let Some(index) = indexes.get(&key).copied() {
            groups[index].count += 1;
        } else {
            indexes.insert(key, groups.len());
            groups.push(AuditRuleGroup {
                rule,
                severity,
                count: 1,
                top_path: path,
            });
        }
    }

    groups.sort_by(|left, right| {
        audit_severity_rank(&left.severity)
            .cmp(&audit_severity_rank(&right.severity))
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.rule.cmp(&right.rule))
            .then_with(|| left.top_path.cmp(&right.top_path))
    });
    groups
}

fn deduped_audit_rows(payload: &Value) -> Vec<Value> {
    let mut indexes = BTreeMap::<String, usize>::new();
    let mut entries = Vec::<(Vec<Value>, usize)>::new();

    for finding in payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = audit_dedupe_key(finding);
        if let Some(index) = indexes.get(&key).copied() {
            entries[index].1 += 1;
            continue;
        }

        indexes.insert(key, entries.len());
        entries.push((
            vec![
                get(finding, "severity"),
                get(finding, "rule"),
                get(finding, "path"),
                get(finding, "line_start"),
                get(finding, "title"),
            ],
            1,
        ));
    }

    entries
        .into_iter()
        .map(|(mut values, instances)| {
            values.push(n(instances));
            row(values)
        })
        .collect()
}

fn audit_dedupe_key(finding: &Value) -> String {
    [
        get(finding, "id"),
        get(finding, "path"),
        get(finding, "line_start"),
        get(finding, "title"),
    ]
    .into_iter()
    .map(|value| match value {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    })
    .collect::<Vec<_>>()
    .join("\u{1f}")
}

fn audit_grouped_next_steps(payload: &Value) -> Vec<NextStep> {
    const MAX_PER_RULE: usize = 1;
    const MAX_TOTAL: usize = 2;

    let groups = audit_rule_groups(payload);
    let findings = payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let mut steps = Vec::new();
    let mut seen = BTreeSet::new();

    for group in groups {
        if steps.len() >= MAX_TOTAL {
            break;
        }
        let mut group_count = 0;
        let mut group_tools = BTreeSet::new();
        for finding in findings.iter().filter(|finding| {
            finding.get("rule").and_then(Value::as_str) == Some(group.rule.as_str())
                && finding.get("severity").and_then(Value::as_str) == Some(group.severity.as_str())
        }) {
            if group_count >= MAX_PER_RULE || steps.len() >= MAX_TOTAL {
                break;
            }
            for step in audit_next_steps_for_finding(finding) {
                if group_count >= MAX_PER_RULE || steps.len() >= MAX_TOTAL {
                    break;
                }
                if step.tool.is_empty()
                    || should_skip_audit_group_step(&group.rule, &group_tools, &step)
                {
                    continue;
                }
                let args = if step.args.is_null() {
                    "{}".to_string()
                } else {
                    step.args.to_string()
                };
                if !seen.insert((step.tool.clone(), args)) {
                    continue;
                }
                group_tools.insert(step.tool.clone());
                steps.push(step);
                group_count += 1;
            }
        }
    }

    steps
}

fn should_skip_audit_group_step(
    rule: &str,
    group_tools: &BTreeSet<String>,
    step: &NextStep,
) -> bool {
    rule == "dependency.hotspot" && step.tool == "outline" && group_tools.contains("outline")
}

fn audit_next_steps_for_finding(finding: &Value) -> Vec<NextStep> {
    let rule = finding.get("rule").and_then(Value::as_str).unwrap_or("");
    let path = finding.get("path").and_then(Value::as_str).unwrap_or("");
    let reason = audit_next_reason(finding);

    match rule {
        "architecture.cycle" => vec![
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "depends_on" }),
                reason.clone(),
            ),
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "imported_by" }),
                reason,
            ),
        ],
        "dependency.hotspot" => vec![
            NextStep::new("outline", json!({ "path": path }), reason.clone()),
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "imported_by" }),
                reason.clone(),
            ),
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "depends_on" }),
                reason,
            ),
        ],
        "symbol.large" => audit_symbol_large_next_steps(finding, reason),
        "file.large" | "dead_code.candidate" | "dependency.unresolved_import" => {
            audit_existing_next_steps(finding, reason)
        }
        _ => audit_existing_next_steps(finding, reason),
    }
}

fn audit_symbol_large_next_steps(finding: &Value, reason: String) -> Vec<NextStep> {
    let mut steps = audit_existing_next_steps(finding, reason);
    for step in &mut steps {
        if step.tool != "read" {
            continue;
        }
        let Some(args) = step.args.as_object_mut() else {
            continue;
        };
        let line_start = args
            .get("line_start")
            .and_then(Value::as_u64)
            .or_else(|| finding.get("line_start").and_then(Value::as_u64));
        let line_end = args
            .get("line_end")
            .and_then(Value::as_u64)
            .or_else(|| finding.get("line_end").and_then(Value::as_u64));

        if let (Some(line_start), Some(line_end)) = (line_start, line_end) {
            args.insert("line_start".to_string(), json!(line_start));
            args.insert(
                "line_end".to_string(),
                json!(line_end.min(line_start.saturating_add(199))),
            );
        }
    }
    steps
}

fn audit_existing_next_steps(finding: &Value, reason: String) -> Vec<NextStep> {
    finding
        .get("next_steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            Some(NextStep::new(
                step.get("tool").and_then(Value::as_str)?,
                step.get("args").cloned().unwrap_or_else(|| json!({})),
                reason.clone(),
            ))
        })
        .collect()
}

fn audit_next_reason(finding: &Value) -> String {
    let rule = finding
        .get("rule")
        .and_then(Value::as_str)
        .unwrap_or("audit");
    let title = finding
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("finding");
    format!("{rule}: {title}")
}

fn audit_severity_rank(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "warning" => 1,
        _ => 2,
    }
}

fn pick(payload: &Value, keys: &[&str]) -> Value {
    let mut map = Map::new();
    for key in keys {
        insert_if_kept(&mut map, key, get(payload, key));
    }
    Value::Object(map)
}

fn drop_false_defaults(value: Value) -> Value {
    let Value::Object(mut map) = value else {
        return value;
    };
    for key in [
        "compact",
        "paths_only",
        "regex",
        "scope",
        "transitive",
        "truncated",
    ] {
        if map.get(key).and_then(Value::as_bool) == Some(false) {
            map.remove(key);
        }
    }
    Value::Object(map)
}

fn without_keys(payload: &Value, keys: &[&str]) -> Value {
    let Some(obj) = payload.as_object() else {
        return payload.clone();
    };
    let mut map = Map::new();
    for (key, value) in obj {
        if !keys.contains(&key.as_str()) {
            insert_if_kept(&mut map, key, value.clone());
        }
    }
    Value::Object(map)
}

fn get(payload: &Value, key: &str) -> Value {
    payload.get(key).cloned().unwrap_or(Value::Null)
}

fn insert_if_kept(map: &mut Map<String, Value>, key: &str, value: Value) {
    if keep_value(&value) {
        map.insert(key.to_string(), value);
    }
}

fn keep_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        _ => true,
    }
}

fn prune_empty_and_null(value: &mut Value) -> bool {
    match value {
        Value::Array(items) => {
            for item in &mut *items {
                if let Value::Object(_) = item {
                    prune_empty_and_null(item);
                }
            }
            !items.is_empty()
        }
        Value::Object(map) => {
            map.retain(|_, item| prune_empty_and_null(item));
            !map.is_empty()
        }
        _ => keep_value(value),
    }
}

pub fn format_unix_ms_utc(ms: u64) -> String {
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
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
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
    use crate::types::{Symbol, SymbolKind};

    #[test]
    fn formats_unix_milliseconds_as_utc() {
        assert_eq!(format_unix_ms_utc(0), "unknown");
        assert_eq!(format_unix_ms_utc(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(
            format_unix_ms_utc(1_582_934_400_123),
            "2020-02-29T00:00:00.123Z"
        );
        assert_eq!(
            format_unix_ms_utc(1_704_067_199_999),
            "2023-12-31T23:59:59.999Z"
        );
    }

    #[test]
    fn serializes_rich_results_with_and_without_scope() {
        let results = vec![
            RichSearchResult {
                path: "src/main.rs".to_string(),
                line_num: 7,
                line_text: "fn main() {}".to_string(),
                scope: Some(Symbol {
                    name: "main".to_string(),
                    kind: SymbolKind::Function,
                    line_start: 7,
                    line_end: 9,
                    detail: Some("()".to_string()),
                }),
            },
            RichSearchResult {
                path: "README.md".to_string(),
                line_num: 1,
                line_text: "# Lexa".to_string(),
                scope: None,
            },
        ];

        let json = rich_results_json(&results);

        assert_eq!(json[0]["path"], "src/main.rs");
        assert_eq!(json[0]["line"], 7);
        assert_eq!(json[0]["scope"]["name"], "main");
        assert_eq!(json[0]["scope"]["kind"], "function");
        assert_eq!(json[0]["scope"]["detail"], "()");
        assert!(json[1]["scope"].is_null());
    }

    #[test]
    fn files_agent_output_omits_modified_column() {
        let value = agent_result_value(
            "files",
            json!({
                "count": 1,
                "files": [{
                    "path": "src/lib.rs",
                    "language": "rust",
                    "line_count": 7,
                    "byte_size": 120,
                    "symbol_count": 2,
                    "modified_utc": "2026-06-30T00:00:00.000Z"
                }]
            }),
        );

        assert_eq!(value["cols"], json!(["path", "lang", "lines", "symbols"]));
        assert_eq!(value["rows"][0], json!(["src/lib.rs", "rust", 7, 2]));
    }

    #[test]
    fn brief_agent_output_trims_keywords_and_structures_next_steps() {
        let value = agent_result_value(
            "brief",
            json!({
                "task": "create project agent",
                "keywords": ["create", "project", "agent", "agentconfig", "projectagent", "creator", "created", "creating", "extra"],
                "max_results": 5,
                "confidence": "low",
                "suggested_next_steps": [
                    "Run symbol-search for likely symbol names.",
                    "Run text-search for concrete terms from the task."
                ]
            }),
        );

        assert_eq!(value["keyword_count"], 9);
        assert_eq!(value["keywords"].as_array().unwrap().len(), 5);
        assert_eq!(value["next_cols"], json!(["tool", "args"]));
        assert_eq!(value["next"][0][0], "symbol_search");
        assert_eq!(value["next"][0][1]["query"], "create project agent");
        assert_eq!(value["next"][1][0], "text_search");
        assert_eq!(value["next"][1][1]["query"], "create project agent");
    }

    #[test]
    fn word_refs_agent_output_structures_paginated_next_step() {
        let value = agent_result_value(
            "word_refs",
            json!({
                "query": "Agent",
                "count": 1,
                "total": 2,
                "limit": 1,
                "cursor": 0,
                "truncated": true,
                "next_cursor": 1,
                "filters": {
                    "path_prefix": "packages/core",
                    "path_glob": "**/*.rs"
                },
                "kind_facets": [{"kind": "definition", "count": 1}],
                "results": [{
                    "kind": "definition",
                    "path": "packages/core/src/lib.rs",
                    "line_num": 3,
                    "score": 120,
                    "line_text": "struct Agent;"
                }]
            }),
        );

        assert_eq!(value["cols"], json!(["kind", "path", "line", "text"]));
        assert_eq!(value["rows"][0][0], "definition");
        assert_eq!(value["rows"][0][3], "struct Agent;");
        assert!(value.get("kind_facets").is_none());
        assert_eq!(value["next_cols"], json!(["tool", "args"]));
        assert_eq!(value["next"][0][0], "word_refs");
        assert_eq!(value["next"][0][1]["word"], "Agent");
        assert_eq!(value["next"][0][1]["cursor"], 1);
        assert_eq!(value["next"][0][1]["path_prefix"], "packages/core");
        assert_eq!(value["next"][0][1]["path_glob"], "**/*.rs");
    }

    #[test]
    fn audit_agent_output_deduplicates_findings_and_next_steps() {
        let finding = json!({
            "id": "architecture.cycle:src/a.rs",
            "severity": "high",
            "rule": "architecture.cycle",
            "path": "src/a.rs",
            "line_start": 1,
            "title": "Import cycle",
            "next_steps": [{
                "tool": "trace_deps",
                "args": {"path": "src/a.rs"}
            }]
        });
        let value = agent_result_value(
            "audit",
            json!({
                "verdict": "warn",
                "summary": {"findings": 2},
                "findings": [finding.clone(), finding]
            }),
        );

        assert_eq!(
            value["rule_cols"],
            json!(["rule", "severity", "count", "top_path"])
        );
        assert_eq!(
            value["rules"][0],
            json!(["architecture.cycle", "high", 2, "src/a.rs"])
        );
        assert_eq!(
            value["cols"],
            json!(["severity", "rule", "path", "line", "title", "instances"])
        );
        assert_eq!(value["rows"].as_array().unwrap().len(), 1);
        assert_eq!(value["rows"][0][5], 2);
        assert_eq!(value["next"].as_array().unwrap().len(), 1);
        assert_eq!(value["next"][0][0], "trace_deps");
        assert_eq!(value["next"][0][1]["direction"], "depends_on");
    }

    #[test]
    fn audit_agent_output_balances_next_steps_across_rule_groups() {
        let value = agent_result_value(
            "audit",
            json!({
                "verdict": "warn",
                "summary": {"findings": 3},
                "findings": [
                    {
                        "id": "architecture.cycle:src/a.rs",
                        "severity": "high",
                        "rule": "architecture.cycle",
                        "path": "src/a.rs",
                        "line_start": 1,
                        "title": "Import cycle",
                        "next_steps": []
                    },
                    {
                        "id": "dependency.hotspot:src/core.rs",
                        "severity": "warning",
                        "rule": "dependency.hotspot",
                        "path": "src/core.rs",
                        "line_start": 1,
                        "title": "Dependency hotspot",
                        "next_steps": []
                    },
                    {
                        "id": "symbol.large:src/big.rs:10:build",
                        "severity": "warning",
                        "rule": "symbol.large",
                        "path": "src/big.rs",
                        "line_start": 10,
                        "line_end": 500,
                        "title": "Large function `build`",
                        "next_steps": [
                            {
                                "tool": "read",
                                "args": {
                                    "path": "src/big.rs",
                                    "line_start": 10,
                                    "line_end": 500
                                }
                            },
                            {
                                "tool": "callers",
                                "args": {"name": "build"}
                            }
                        ]
                    }
                ]
            }),
        );

        let next = value["next"].as_array().unwrap();
        assert_eq!(next.len(), 2);
        assert_eq!(next[0][0], "trace_deps");
        assert_eq!(next[0][1]["path"], "src/a.rs");
        assert_eq!(next[0][1]["direction"], "depends_on");
        assert!(next
            .iter()
            .any(|row| { row[0] == "outline" && row[1]["path"] == "src/core.rs" }));
    }

    #[test]
    fn audit_agent_output_falls_back_when_group_next_steps_dedupe() {
        let value = agent_result_value(
            "audit",
            json!({
                "verdict": "warn",
                "summary": {"findings": 4},
                "findings": [
                    {
                        "id": "architecture.cycle:src/shared.ts",
                        "severity": "high",
                        "rule": "architecture.cycle",
                        "path": "src/shared.ts",
                        "title": "Import cycle",
                        "next_steps": []
                    },
                    {
                        "id": "architecture.cycle:src/other.ts",
                        "severity": "high",
                        "rule": "architecture.cycle",
                        "path": "src/other.ts",
                        "title": "Import cycle",
                        "next_steps": []
                    },
                    {
                        "id": "dependency.hotspot:src/shared.ts",
                        "severity": "high",
                        "rule": "dependency.hotspot",
                        "path": "src/shared.ts",
                        "title": "Dependency hotspot",
                        "next_steps": []
                    },
                    {
                        "id": "dependency.hotspot:src/core.ts",
                        "severity": "high",
                        "rule": "dependency.hotspot",
                        "path": "src/core.ts",
                        "title": "Dependency hotspot",
                        "next_steps": []
                    }
                ]
            }),
        );

        let next = value["next"].as_array().unwrap();
        assert_eq!(next.len(), 2);
        assert!(next
            .iter()
            .any(|row| { row[0] == "outline" && row[1]["path"] == "src/shared.ts" }));
        let hotspot_outlines = next
            .iter()
            .filter(|row| {
                row[0] == "outline"
                    && matches!(
                        row[1]["path"].as_str(),
                        Some("src/shared.ts" | "src/core.ts")
                    )
            })
            .count();
        assert_eq!(hotspot_outlines, 1);
    }

    #[test]
    fn glob_agent_output_uses_direct_paths_and_compresses_root() {
        let value = agent_result_value(
            "glob",
            json!({
                "pattern": "src/*.ts",
                "count": 4,
                "paths": [
                    "src/app.ts",
                    "src/web_agent.ts",
                    "src/web_config.ts",
                    "src/web_runtime.ts"
                ]
            }),
        );

        assert_eq!(value["root"], "src/");
        assert!(value.get("cols").is_none());
        assert_eq!(
            value["paths"],
            json!(["app.ts", "web_agent.ts", "web_config.ts", "web_runtime.ts"])
        );
    }

    #[test]
    fn search_agent_output_trims_long_text_cells() {
        let value = agent_result_value(
            "text_search",
            json!({
                "query": "needle",
                "count": 1,
                "limit": 1,
                "results": [{
                    "path": "src/main.rs",
                    "line_num": 7,
                    "line_text": "needle   followed by a very long line that keeps going and going and going and going and going and going and going and going and going"
                }]
            }),
        );

        let text = value["rows"][0][2].as_str().unwrap();
        assert!(text.len() <= 120);
        assert!(text.contains("needle followed"));
        assert!(text.ends_with("..."));
    }

    #[test]
    fn create_agent_output_keeps_would_create_semantics() {
        let value = agent_result_value(
            "create",
            json!({
                "path": "src/new.rs",
                "op": "create",
                "dry_run": true,
                "changed": false,
                "would_create": true,
                "hash": "abc123",
                "line_count": 1,
                "byte_size": 12
            }),
        );

        assert_eq!(value["would_create"], true);
        assert!(value.get("next").is_none());
    }
}
