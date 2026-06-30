#![allow(clippy::unwrap_used)]

mod common;

use common::{
    assert_all_correct, bench_result_against, parse_json, parse_toon, print_report, run_lexa,
    write_fixture, BenchResult,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

const SUITE: &str = "mcp_session";

#[test]
fn agent_mcp_session_benchmark_v2() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    write_fixture(project);
    run_lexa(project, &["index", "."]);
    std::thread::sleep(Duration::from_millis(20));

    let mut session = McpSession::start(project);
    session.initialize();

    let results = vec![
        mcp_patch_task(&mut session, project),
        mcp_create_task(&mut session, project),
        mcp_changes_task(&mut session),
        mcp_recent_task(&mut session),
        mcp_status_task(&mut session),
    ];

    print_report(SUITE, &results);
    assert_all_correct(&results);
}

fn mcp_patch_task(session: &mut McpSession, project: &Path) -> BenchResult {
    let response = session.call_tool(
        "patch",
        json!({
            "path": "src/app.rs",
            "op": "insert",
            "after": 1,
            "content": "fn session_patch_marker() {}"
        }),
    );
    let text = tool_text(&response);
    let payload = tool_payload(&response);
    let content = std::fs::read_to_string(project.join("src/app.rs")).unwrap();
    let correct = payload["tool"] == "patch"
        && payload["path"] == "src/app.rs"
        && payload["changed"] == true
        && payload.get("hash").is_some()
        && content.contains("session_patch_marker");
    bench_result_against(
        SUITE,
        "persistent patch",
        "patch",
        "Lexa MCP session patch",
        text,
        None,
        correct,
    )
}

fn mcp_create_task(session: &mut McpSession, project: &Path) -> BenchResult {
    let response = session.call_tool(
        "create",
        json!({
            "path": "src/session_created.rs",
            "content": "pub fn session_created() -> usize { 11 }\n"
        }),
    );
    let text = tool_text(&response);
    let payload = tool_payload(&response);
    let content = std::fs::read_to_string(project.join("src/session_created.rs")).unwrap();
    let correct = payload["tool"] == "create"
        && payload["path"] == "src/session_created.rs"
        && payload["changed"] == true
        && content.contains("session_created");
    bench_result_against(
        SUITE,
        "persistent create",
        "create",
        "Lexa MCP session create",
        text,
        None,
        correct,
    )
}

fn mcp_changes_task(session: &mut McpSession) -> BenchResult {
    let response = session.call_tool("changes", json!({"since": 0}));
    let text = tool_text(&response);
    let correct = text.contains("src/app.rs")
        && text.contains("Insert")
        && text.contains("src/session_created.rs")
        && text.contains("Create");
    bench_result_against(
        SUITE,
        "session change history",
        "changes",
        "Lexa MCP session change log",
        text,
        None,
        correct,
    )
}

fn mcp_recent_task(session: &mut McpSession) -> BenchResult {
    let response = session.call_tool("recent", json!({"limit": 5}));
    let text = tool_text(&response);
    let payload = parse_json(text);
    let correct = payload["files"].as_array().is_some_and(|files| {
        files
            .iter()
            .any(|file| file["path"] == "src/session_created.rs")
    });
    bench_result_against(
        SUITE,
        "session recent state",
        "recent",
        "Lexa MCP session recent state",
        text,
        None,
        correct,
    )
}

fn mcp_status_task(session: &mut McpSession) -> BenchResult {
    let response = session.call_tool("status", json!({}));
    let text = tool_text(&response);
    let payload = tool_payload(&response);
    let correct = payload["seq"].as_u64().unwrap() >= 2
        && payload["graph"]["exists"] == true
        && payload["change_history_persisted"] == false;
    bench_result_against(
        SUITE,
        "session status state",
        "status",
        "Lexa MCP session status state",
        text,
        None,
        correct,
    )
}

fn tool_text(response: &Value) -> &str {
    assert_eq!(response["result"]["isError"], false, "{response}");
    response["result"]["content"][0]["text"].as_str().unwrap()
}

fn tool_payload(response: &Value) -> Value {
    parse_toon(tool_text(response))
}

struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpSession {
    fn start(project: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lexa"))
            .current_dir(project)
            .args(["mcp", "."])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn initialize(&mut self) {
        let id = self.next_id();
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "lexa-benchmark",
                    "version": "0.0.0"
                }
            }
        }));
        assert_eq!(response["id"], id);
        assert_eq!(response["result"]["serverInfo"]["name"], "lexa");
        self.notify(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }));
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id();
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }));
        assert_eq!(response["id"], id);
        response
    }

    fn request(&mut self, value: Value) -> Value {
        self.write_json(&value);
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "MCP server closed stdout");
        parse_json(&line)
    }

    fn notify(&mut self, value: Value) {
        self.write_json(&value);
    }

    fn write_json(&mut self, value: &Value) {
        writeln!(self.stdin, "{}", value).unwrap();
        self.stdin.flush().unwrap();
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
