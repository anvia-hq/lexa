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
