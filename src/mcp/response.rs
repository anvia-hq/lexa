use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::tool_spec;
use crate::output::{agent_error_toon, agent_toon};

pub(super) struct ToolOutput {
    pub(super) structured: Value,
}

impl ToolOutput {
    pub(super) fn new(structured: Value) -> Self {
        Self { structured }
    }
}

pub(super) fn tool_response(id: Value, name: &str, result: Result<ToolOutput>) -> Value {
    match result {
        Ok(output) => match agent_toon(name, output.structured) {
            Ok(text) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": text }],
                    "isError": false
                }
            }),
            Err(err) => {
                let text = encode_tool_error(name, &err.to_string());
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": text }],
                        "isError": true
                    }
                })
            }
        },
        Err(err) => {
            let text = encode_tool_error(name, &err.to_string());
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": text }],
                    "isError": true
                }
            })
        }
    }
}

pub(super) fn encode_tool_error(name: &str, error: &str) -> String {
    agent_error_toon(name, error)
        .unwrap_or_else(|err| format!("tool: {}\nok: false\nerror: {err}", name.replace('-', "_")))
}

pub(super) fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message }
    })
}

pub(super) fn graph_status(path: &PathBuf) -> Value {
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

pub(super) fn tools() -> Value {
    json!(tool_spec::TOOL_SPECS
        .iter()
        .map(|spec| json!({
            "name": spec.name,
            "description": spec.summary,
            "inputSchema": compact_input_schema(&spec.input_schema),
        }))
        .collect::<Vec<_>>())
}

pub(super) fn compact_input_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !matches!(key.as_str(), "description" | "examples"))
                .map(|(key, value)| (key.clone(), compact_input_schema(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(compact_input_schema).collect()),
        _ => schema.clone(),
    }
}
