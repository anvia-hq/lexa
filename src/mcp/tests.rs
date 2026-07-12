use super::response::*;
use super::server::McpServer;
use super::tool_spec;
use super::transport::*;
use super::Diagnostics;
use crate::engine::Engine;
use anyhow::Result;
use serde_json::{json, Value};
use std::io::Cursor;

fn server_for_root(root: &tempfile::TempDir) -> McpServer {
    McpServer::new(
        Engine::new(32),
        root.path().to_path_buf(),
        root.path().join(".lexa/graph.lexa"),
        false,
        Diagnostics::disabled(),
    )
}

fn indexed_server(root: &tempfile::TempDir, files: &[(&str, &str)]) -> McpServer {
    let mut engine = Engine::new(64);
    for (path, content) in files {
        let abs = root.path().join(path);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&abs, content).unwrap();
        engine.index_file(path, content);
    }
    McpServer::new(
        engine,
        root.path().to_path_buf(),
        root.path().join(".lexa/graph.lexa"),
        false,
        Diagnostics::disabled(),
    )
}

fn tool_err(result: Result<ToolOutput>) -> String {
    match result {
        Ok(_) => panic!("expected tool error"),
        Err(err) => err.to_string(),
    }
}

fn schema_contains_key(value: &Value, needle: &str) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, value)| key == needle || schema_contains_key(value, needle)),
        Value::Array(items) => items.iter().any(|value| schema_contains_key(value, needle)),
        _ => false,
    }
}

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
fn read_message_rejects_oversized_content_length_before_reading_body() {
    let framed = format!("Content-Length: {}\r\n\r\n", MAX_MCP_MESSAGE_BYTES + 1);
    let mut reader = Cursor::new(framed.into_bytes());

    let err = match read_message(&mut reader) {
        Ok(_) => panic!("expected oversized message error"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("exceeds maximum MCP message size"));
}

#[test]
fn read_message_rejects_oversized_header_lines() {
    let mut input = vec![b'x'; MAX_MCP_HEADER_BYTES + 1];
    input.push(b'\n');
    let mut reader = Cursor::new(input);

    let err = match read_message(&mut reader) {
        Ok(_) => panic!("expected oversized header error"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("headers exceed maximum size"));
}

#[test]
fn limited_line_reader_rejects_oversized_newline_frames() {
    let mut reader = Cursor::new(b"12345\n".to_vec());

    let err = read_non_empty_line(&mut reader, 4).unwrap_err();

    assert!(err.to_string().contains("line exceeds maximum size"));
}

#[test]
fn read_message_handles_non_utf8_input_without_io_utf8_error() {
    let mut reader = Cursor::new(vec![0xff, b'\n']);

    assert!(read_message(&mut reader).unwrap().is_none());
}

#[test]
fn diagnostics_create_plain_text_log_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lexa-mcp.log");

    let mut diagnostics = Diagnostics::append_to_path(&path).unwrap();
    diagnostics.info("starting MCP server");
    drop(diagnostics);

    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains(" INFO starting MCP server\n"));
}

#[test]
fn diagnostics_append_to_existing_log_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lexa-mcp.log");
    std::fs::write(&path, "existing\n").unwrap();

    let mut diagnostics = Diagnostics::append_to_path(&path).unwrap();
    diagnostics.warn("watch error");
    drop(diagnostics);

    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.starts_with("existing\n"));
    assert!(content.contains(" WARN watch error\n"));
}

#[test]
fn diagnostics_report_invalid_log_path() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("missing").join("lexa-mcp.log");

    let err = match Diagnostics::append_to_path(&path) {
        Ok(_) => panic!("expected invalid log path error"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("failed to open MCP log file"));
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

fn decode_response_text(response: &Value) -> Value {
    toon_format::decode_default::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
        .unwrap()
}

#[test]
fn tool_response_returns_toon_text_without_structured_content() {
    let response = tool_response(
        json!(1),
        "status",
        Ok(ToolOutput::new(json!({"files_indexed": 1}))),
    );

    let decoded = decode_response_text(&response);
    assert_eq!(decoded["tool"], "status");
    assert!(decoded.get("ok").is_none());
    assert_eq!(decoded["files_indexed"], 1);
    assert!(response["result"].get("structuredContent").is_none());
}

#[test]
fn tool_error_response_returns_toon_text_without_structured_content() {
    let response = tool_response(json!(1), "status", Err(anyhow::anyhow!("bad input")));

    let decoded = decode_response_text(&response);
    assert_eq!(response["result"]["isError"], Value::Bool(true));
    assert_eq!(decoded["tool"], "status");
    assert_eq!(decoded["ok"], false);
    assert_eq!(decoded["error"], "bad input");
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
fn tools_list_shape_matches_table() {
    let tools = tools();
    let arr = tools.as_array().expect("tools list must be an array");

    assert_eq!(arr.len(), tool_spec::TOOL_SPECS.len());
    for (tool, spec) in arr.iter().zip(tool_spec::TOOL_SPECS.iter()) {
        assert_eq!(tool["name"], spec.name);
        assert_eq!(tool["description"], spec.summary);
        assert_eq!(
            tool["inputSchema"],
            compact_input_schema(&spec.input_schema)
        );
    }
}

#[test]
fn tools_list_compacts_nested_schema_descriptions() {
    let tools = tools();
    let arr = tools.as_array().expect("tools list must be an array");

    for tool in arr {
        let schema = &tool["inputSchema"];
        assert!(
            !schema_contains_key(schema, "description"),
            "{} schema still contains description fields",
            tool["name"].as_str().unwrap_or("<unknown>")
        );
        assert!(
            !schema_contains_key(schema, "examples"),
            "{} schema still contains examples fields",
            tool["name"].as_str().unwrap_or("<unknown>")
        );
    }
}

#[test]
fn outline_missing_file_reports_clean_error() {
    let root = tempfile::tempdir().unwrap();
    let server = server_for_root(&root);

    let err = match server.tool_outline("apps/desktop/CLAUDE.md") {
        Ok(_) => panic!("expected missing file error"),
        Err(err) => err.to_string(),
    };

    assert_eq!(err, "file not found: apps/desktop/CLAUDE.md");
    assert!(!err.contains(root.path().to_string_lossy().as_ref()));
    assert!(!err.contains("canonicalize"));
}

#[test]
fn word_refs_supports_filters_and_source_ranked_pagination() {
    let root = tempfile::tempdir().unwrap();
    let server = indexed_server(
        &root,
        &[
            ("docs/agent.md", "Agent docs\n"),
            ("examples/agent.rs", "let _ = Agent;\n"),
            ("packages/core/src/agent.rs", "struct Agent;\n"),
            (
                "packages/core/src/use_agent.ts",
                "import { Agent } from './agent';\n",
            ),
            (
                "packages/core/test/helpers/imports.ts",
                "export { Agent } from '../../src/agent';\n",
            ),
            ("packages/core/tests/agent_test.rs", "Agent::new();\n"),
        ],
    );

    let first_page = server
        .tool_find_word(&json!({
            "word": "Agent",
            "path_prefix": "packages/core",
            "max_results": 1
        }))
        .unwrap();

    assert_eq!(first_page.structured["total"], 4);
    assert_eq!(
        first_page.structured["results"][0]["path"],
        "packages/core/src/agent.rs"
    );
    assert_eq!(first_page.structured["results"][0]["kind"], "definition");
    assert!(
        first_page.structured["results"][0]["score"]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(first_page.structured["kind_facets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|facet| facet["kind"] == "definition"));
    assert_eq!(first_page.structured["next_cursor"], 1);
    assert_eq!(
        first_page.structured["filters"]["path_prefix"],
        "packages/core"
    );

    let zero_limit = server
        .tool_find_word(&json!({
            "word": "Agent",
            "path_prefix": "packages/core",
            "max_results": 0
        }))
        .unwrap();
    assert_eq!(zero_limit.structured["limit"], 1);
    assert_eq!(zero_limit.structured["count"], 1);
    assert_eq!(zero_limit.structured["next_cursor"], 1);

    let ranked = server
        .tool_find_word(&json!({
            "word": "Agent",
            "path_prefix": "packages/core",
            "max_results": 4
        }))
        .unwrap();
    let ranked_results = ranked.structured["results"].as_array().unwrap();
    let source_import_index = ranked_results
        .iter()
        .position(|result| result["path"] == "packages/core/src/use_agent.ts")
        .unwrap();
    let test_export_index = ranked_results
        .iter()
        .position(|result| result["path"] == "packages/core/test/helpers/imports.ts")
        .unwrap();
    assert!(source_import_index < test_export_index);
    assert_eq!(ranked_results[test_export_index]["kind"], "export");
    assert!(
        ranked_results[source_import_index]["score"]
            .as_i64()
            .unwrap()
            > ranked_results[test_export_index]["score"].as_i64().unwrap()
    );

    let globbed = server
        .tool_find_word(&json!({
            "query": "Agent",
            "path_glob": "**/tests/*.rs"
        }))
        .unwrap();

    assert_eq!(globbed.structured["total"], 1);
    assert_eq!(
        globbed.structured["results"][0]["path"],
        "packages/core/tests/agent_test.rs"
    );
    assert_eq!(globbed.structured["results"][0]["kind"], "test");
}

#[test]
fn read_tool_returns_hash_content_ranges_and_unchanged_response() {
    let root = tempfile::tempdir().unwrap();
    let mut server = indexed_server(&root, &[("src/app.rs", "one\ntwo\nthree\n")]);

    let full = server.tool_read(&json!({"path": "src/app.rs"})).unwrap();
    let hash = full.structured["hash"].as_str().unwrap().to_string();
    assert_eq!(full.structured["content"], "one\ntwo\nthree\n");
    assert_eq!(full.structured["unchanged"], false);

    let ranged = server
        .tool_read(&json!({
            "path": "src/app.rs",
            "line_start": 2,
            "line_end": 2,
            "compact": true
        }))
        .unwrap();
    assert_eq!(ranged.structured["line_start"], 2);
    assert_eq!(ranged.structured["line_end"], 2);
    assert_eq!(ranged.structured["compact"], true);
    assert!(ranged.structured["content"]
        .as_str()
        .unwrap()
        .contains("two"));

    let unchanged = server
        .tool_read(&json!({"path": "src/app.rs", "if_hash": hash, "compact": true}))
        .unwrap();
    assert_eq!(unchanged.structured["unchanged"], true);
    assert_eq!(unchanged.structured["compact"], true);
    assert_eq!(unchanged.structured["content"], "");
}

#[test]
fn patch_tool_supports_dry_run_real_change_unchanged_and_errors() {
    let root = tempfile::tempdir().unwrap();
    let mut server = indexed_server(&root, &[("src/app.rs", "one\ntwo\n")]);

    let dry_run = server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "op": "replace",
            "range_start": 2,
            "content": "TWO",
            "dry_run": true
        }))
        .unwrap();
    assert_eq!(dry_run.structured["dry_run"], true);
    assert_eq!(dry_run.structured["changed"], true);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/app.rs")).unwrap(),
        "one\ntwo\n"
    );

    let changed = server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "op": "replace",
            "range_start": 2,
            "content": "TWO"
        }))
        .unwrap();
    assert_eq!(changed.structured["changed"], true);
    assert_eq!(changed.structured["op"], "replace");
    assert_eq!(changed.structured["change_sequence"], 2);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/app.rs")).unwrap(),
        "one\nTWO\n"
    );
    assert!(server
        .engine
        .read_file("src/app.rs", None, None)
        .unwrap()
        .contains("TWO"));

    let unchanged = server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "op": "replace",
            "range_start": 2,
            "content": "TWO"
        }))
        .unwrap();
    assert_eq!(unchanged.structured["changed"], false);
    assert_eq!(unchanged.structured["line_count"], 2);

    let bad_op = tool_err(server.tool_patch(&json!({"path": "src/app.rs", "op": "move"})));
    assert!(bad_op.contains("op must be replace, insert, or delete"));

    let bad_hash = tool_err(server.tool_patch(&json!({
        "path": "src/app.rs",
        "op": "delete",
        "range_start": 1,
        "if_hash": "bad"
    })));
    assert!(bad_hash.contains("hash mismatch"));
}

#[test]
fn patch_tool_supports_replace_text_anchor_and_preview_mode() {
    let root = tempfile::tempdir().unwrap();
    let mut server = indexed_server(&root, &[("src/app.rs", "one\ntwo\nthree\n")]);

    let dry_run = server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "replace_text": "two",
            "content": "TWO",
            "dry_run": true,
            "preview_mode": "compact"
        }))
        .unwrap();
    assert_eq!(dry_run.structured["op"], "replace-text");
    assert_eq!(dry_run.structured["preview_mode"], "compact");
    assert_eq!(dry_run.structured["lines_added"], 1);
    assert_eq!(dry_run.structured["lines_removed"], 1);

    server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "replace_text": "two",
            "content": "TWO"
        }))
        .unwrap();

    let unchanged = server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "replace_text": "TWO",
            "content": "TWO"
        }))
        .unwrap();
    assert_eq!(unchanged.structured["op"], "replace-text");
    assert_eq!(unchanged.structured["changed"], false);

    let anchor = server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "anchor": "TWO",
            "placement": "after",
            "content": "inserted"
        }))
        .unwrap();
    assert_eq!(anchor.structured["op"], "anchor");
    assert_eq!(anchor.structured["lines_added"], 1);
    assert_eq!(anchor.structured["lines_removed"], 0);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/app.rs")).unwrap(),
        "one\nTWO\ninserted\nthree\n"
    );

    std::fs::write(root.path().join("src/app.rs"), "same\nsame\n").unwrap();
    let err = tool_err(server.tool_patch(&json!({
        "path": "src/app.rs",
        "replace_text": "same",
        "content": "changed"
    })));
    assert!(err.contains("matched multiple locations"));
}

#[test]
fn create_tool_supports_dry_run_real_create_and_overwrite_rules() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    let mut server = server_for_root(&root);

    let dry_run = server
        .tool_create(&json!({
            "path": "src/new.rs",
            "content": "fn new() {}\n",
            "dry_run": true
        }))
        .unwrap();
    assert_eq!(dry_run.structured["dry_run"], true);
    assert_eq!(dry_run.structured["changed"], false);
    assert_eq!(dry_run.structured["would_create"], true);
    assert!(!root.path().join("src/new.rs").exists());

    let created = server
        .tool_create(&json!({
            "path": "src/new.rs",
            "content": "fn new() {}\n"
        }))
        .unwrap();
    assert_eq!(created.structured["changed"], true);
    assert_eq!(created.structured["change_sequence"], 1);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/new.rs")).unwrap(),
        "fn new() {}\n"
    );
    assert!(server
        .engine
        .read_file("src/new.rs", None, None)
        .unwrap()
        .contains("fn new"));

    let exists = tool_err(server.tool_create(&json!({
        "path": "src/new.rs",
        "content": "blocked"
    })));
    assert!(exists.contains("file already exists"));

    let overwritten = server
        .tool_create(&json!({
            "path": "src/new.rs",
            "content": "fn updated() {}\n",
            "overwrite": true
        }))
        .unwrap();
    assert_eq!(overwritten.structured["changed"], true);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/new.rs")).unwrap(),
        "fn updated() {}\n"
    );
}

#[test]
fn changes_recent_and_status_reflect_session_state() {
    let root = tempfile::tempdir().unwrap();
    let mut server = indexed_server(&root, &[("src/app.rs", "fn app() {}\n")]);
    let since = server.engine.store().current_seq();

    let empty_changes = server.tool_changes(since);
    assert_eq!(empty_changes.structured["count"], 0);

    server
        .tool_patch(&json!({
            "path": "src/app.rs",
            "op": "insert",
            "after": 1,
            "content": "fn next() {}"
        }))
        .unwrap();

    let changes = server.tool_changes(since);
    assert_eq!(changes.structured["count"], 1);
    assert_eq!(changes.structured["changes"][0]["path"], "src/app.rs");
    assert_eq!(changes.structured["change_history_persisted"], false);

    let recent = server.tool_recent(5);
    assert_eq!(recent.structured["count"], 1);
    assert_eq!(recent.structured["files"][0]["path"], "src/app.rs");

    let status = server.tool_status();
    assert_eq!(status.structured["files_indexed"], 1);
    assert_eq!(status.structured["seq"], since + 1);
    assert_eq!(status.structured["change_history_persisted"], false);
    assert_eq!(status.structured["graph"]["exists"], false);
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
        Diagnostics::disabled(),
    );

    let output = server
        .tool_pipeline(&json!({
        "query": "ignored",
        "steps": ["search AgentRunRequest", "limit 3"]
        }))
        .unwrap();

    assert_eq!(output.structured["result_type"], "results");
    assert!(output.structured["items"]
        .to_string()
        .contains("AgentRunRequest"));
}

#[test]
fn pipeline_query_only_is_not_supported() {
    let root = tempfile::tempdir().unwrap();
    let server = McpServer::new(
        Engine::new(32),
        root.path().to_path_buf(),
        root.path().join(".lexa/graph.lexa"),
        false,
        Diagnostics::disabled(),
    );

    let err = match server.tool_pipeline(&json!({"query": "search AgentRunRequest | limit 1"})) {
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
