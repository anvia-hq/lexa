use super::diagnostics::Diagnostics;
use super::response::json_rpc_error;
use super::transport::{read_message, write_response};
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

use crate::engine::Engine;
use crate::{freshness, snapshot};

pub struct McpServer {
    pub(super) engine: Engine,
    pub(super) root: PathBuf,
    pub(super) graph_path: PathBuf,
    pub(super) persist_graph: bool,
    diagnostics: Diagnostics,
    watcher: Option<RuntimeWatcher>,
}

struct RuntimeWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
}

impl McpServer {
    pub fn new(
        engine: Engine,
        root: PathBuf,
        graph_path: PathBuf,
        persist_graph: bool,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            engine,
            root,
            graph_path,
            persist_graph,
            diagnostics,
            watcher: None,
        }
    }

    pub fn enable_watcher(&mut self, debounce_ms: u64) -> Result<()> {
        let (tx, rx) = channel();
        let mut watcher = match RecommendedWatcher::new(
            tx,
            notify::Config::default().with_poll_interval(Duration::from_millis(debounce_ms)),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                self.diagnostics
                    .error(format!("failed to create MCP watcher: {err}"));
                return Err(err.into());
            }
        };
        if let Err(err) = watcher.watch(&self.root, RecursiveMode::Recursive) {
            self.diagnostics.error(format!(
                "failed to watch {} for MCP graph freshness: {err}",
                self.root.display()
            ));
            return Err(err.into());
        }
        let message = format!("Watching {} for MCP graph freshness", self.root.display());
        eprintln!("{message}");
        self.diagnostics.info(message);
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

        loop {
            let message = match read_message(&mut reader) {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(err) => {
                    self.diagnostics
                        .error(format!("failed to read MCP message: {err}"));
                    return Err(err);
                }
            };
            let request: Value = match serde_json::from_slice(&message.body) {
                Ok(value) => value,
                Err(err) => {
                    self.diagnostics
                        .error(format!("failed to parse MCP request: {err}"));
                    if let Err(write_err) = write_response(
                        &mut writer,
                        message.framing,
                        &json_rpc_error(None, -32700, &err.to_string()),
                    ) {
                        self.diagnostics.error(format!(
                            "failed to write MCP parse error response: {write_err}"
                        ));
                        return Err(write_err);
                    }
                    continue;
                }
            };

            if let Err(err) = self.refresh_from_watcher() {
                self.diagnostics
                    .error(format!("failed to refresh MCP graph from watcher: {err}"));
                return Err(err);
            }

            let id = request.get("id").cloned();
            let Some(method) = request.get("method").and_then(Value::as_str) else {
                if id.is_some() {
                    if let Err(err) = write_response(
                        &mut writer,
                        message.framing,
                        &json_rpc_error(id, -32600, "missing JSON-RPC method"),
                    ) {
                        self.diagnostics
                            .error(format!("failed to write MCP error response: {err}"));
                        return Err(err);
                    }
                }
                continue;
            };

            let Some(response) = self.handle(method, id, request.get("params")) else {
                continue;
            };
            if let Err(err) = write_response(&mut writer, message.framing, &response) {
                self.diagnostics
                    .error(format!("failed to write MCP response: {err}"));
                return Err(err);
            }
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
                Ok(Err(err)) => {
                    eprintln!("Watch error: {err}");
                    self.diagnostics.warn(format!("watch error: {err}"));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if paths.is_empty() {
            return Ok(());
        }

        let message = format!(
            "Checking MCP graph freshness for {} changed path(s)",
            paths.len()
        );
        eprintln!("{message}");
        self.diagnostics.info(message);

        let summary = match freshness::refresh_paths(&mut self.engine, &self.root, paths) {
            Ok(summary) => summary,
            Err(err) => {
                self.diagnostics.error(format!(
                    "failed to refresh changed paths for {}: {err}",
                    self.root.display()
                ));
                return Err(err);
            }
        };
        if summary.changed() {
            if self.persist_graph {
                if let Err(err) = snapshot::write_snapshot(&self.engine, &self.graph_path) {
                    self.diagnostics.error(format!(
                        "failed to save MCP graph {}: {err}",
                        self.graph_path.display()
                    ));
                    return Err(err);
                }
            }
            let message = format!(
                "MCP graph refreshed: {} indexed, {} removed",
                summary.indexed, summary.removed
            );
            eprintln!("{message}");
            self.diagnostics.info(message);
        }

        Ok(())
    }
}
