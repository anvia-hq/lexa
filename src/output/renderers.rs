use super::compact::{
    apply_path_compression, file_row, insert_rows, kind_value, line_value, search_row, text_value,
    with_rows, with_rows_map, word_ref_row, PathTarget,
};
use super::guidance::{
    audit_grouped_next_steps, audit_rule_groups, brief_next_steps, deduped_audit_rows,
    insert_audit_rule_rows, insert_next_steps, should_emit_brief_next, trim_summary_keywords,
    NextStep,
};
use super::value::*;
use serde_json::{json, Map, Value};

pub(super) fn files_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn list_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn glob_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn path_search_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn outline_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn symbol_defs_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn symbol_search_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn word_refs_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn search_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn callers_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn brief_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn trace_deps_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn read_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn edit_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn changes_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn recent_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn summary_only_result(tool: &str, payload: Value) -> Value {
    Value::Object(base(tool, without_keys(&payload, &["refresh"])))
}

pub(super) fn audit_result(tool: &str, payload: Value) -> Value {
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

pub(super) fn pipeline_result(tool: &str, payload: Value) -> Value {
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
