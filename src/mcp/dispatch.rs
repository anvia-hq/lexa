use super::args::*;
use super::response::{json_rpc_error, tool_response, tools, ToolOutput};
use super::server::McpServer;
use super::transport::requested_protocol_version;
use anyhow::{bail, Result};
use serde_json::{json, Value};

const MAX_RETRIEVAL_RESULTS: usize = 200;

impl McpServer {
    pub(super) fn handle(
        &mut self,
        method: &str,
        id: Option<Value>,
        params: Option<&Value>,
    ) -> Option<Value> {
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
                Some(tool_response(id, name, result))
            }
            "ping" => id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": {} })),
            _ => id.map(|id| json_rpc_error(Some(id), -32601, "method not found")),
        }
    }

    pub(super) fn call_tool(&mut self, name: &str, args: &Value) -> Result<ToolOutput> {
        match name {
            "files" => Ok(self.tool_map(args)),
            "list" => Ok(self.tool_list(opt_str(args, "path").unwrap_or(""))),
            "glob" => self.tool_glob(req_str(args, "pattern")?),
            "path_search" => self.tool_find_path(
                req_any_str(args, &["query", "path", "pattern", "name"])?,
                opt_usize(args, "max_results")
                    .or_else(|| opt_usize(args, "max"))
                    .unwrap_or(20)
                    .min(MAX_RETRIEVAL_RESULTS),
            ),
            "outline" => self.tool_outline(req_str(args, "path")?),
            "symbol_defs" => self.tool_find_symbol(req_any_str(args, &["name", "query"])?),
            "symbol_search" => self.tool_symbol_search(args),
            "word_refs" => self.tool_find_word(args),
            "text_search" => self.tool_search(args),
            "callers" => self.tool_find_callers(req_any_str(args, &["name", "query"])?),
            "brief" => self.tool_brief(args),
            "trace_deps" => self.tool_trace_deps(args),
            "read" => self.tool_read(args),
            "patch" => self.tool_patch(args),
            "create" => self.tool_create(args),
            "changes" => Ok(self.tool_changes(opt_u64(args, "since").unwrap_or(0))),
            "recent" => Ok(self.tool_recent(
                opt_usize(args, "limit")
                    .unwrap_or(10)
                    .min(MAX_RETRIEVAL_RESULTS),
            )),
            "status" => Ok(self.tool_status()),
            "reindex" => self.tool_reindex(),
            "clear_index" => self.tool_clear_index(),
            "audit" => self.tool_audit(args),
            "pipeline" => self.tool_pipeline(args),
            _ => bail!("unknown tool: {name}"),
        }
    }
}
