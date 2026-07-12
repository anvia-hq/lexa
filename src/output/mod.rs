use crate::engine::{RichSearchResult, WordSearchResult};
use crate::types::SearchResult;
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

mod compact;
mod guidance;
mod renderers;
mod time;
mod value;

#[cfg(test)]
mod tests;

pub use time::format_unix_ms_utc;

use renderers::*;
use value::{base, flatten_metadata, prune_empty_and_null, s, without_keys};

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
