use super::args::*;
use super::response::{graph_status, ToolOutput};
use super::server::McpServer;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::application::ProjectSession;
use crate::audit::{self, AuditOptions};
use crate::output::format_unix_ms_utc;

const MAX_AUDIT_RESULTS: usize = 1000;

impl McpServer {
    pub(super) fn tool_changes(&self, since: u64) -> ToolOutput {
        let changes = self.engine.get_changes(since);
        if changes.is_empty() {
            return ToolOutput::new(json!({
                "since": since,
                "count": 0,
                "change_history_persisted": false,
                "note": "Change history is session-local and is not restored from graph snapshots.",
                "changes": []
            }));
        }

        ToolOutput::new(json!({
            "since": since,
            "count": changes.len(),
            "change_history_persisted": false,
            "note": "Change history is session-local and is not restored from graph snapshots.",
            "changes": changes.into_iter().map(|(path, seq, op)| json!({
                "path": path,
                "seq": seq,
                "op": op,
            })).collect::<Vec<_>>()
        }))
    }

    pub(super) fn tool_recent(&self, limit: usize) -> ToolOutput {
        let files = self.engine.get_hot_files(limit);
        if files.is_empty() {
            return ToolOutput::new(json!({"count": 0, "limit": limit, "files": []}));
        }

        ToolOutput::new(json!({
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
        }))
    }

    pub(super) fn tool_status(&self) -> ToolOutput {
        let graph = graph_status(&self.graph_path);
        ToolOutput::new(json!({
            "files_indexed": self.engine.file_count(),
            "symbols_indexed": self.engine.symbol_index_count(),
            "unique_words_indexed": self.engine.word_index_count(),
            "word_indexed_files": self.engine.word_index_file_count(),
            "seq": self.engine.store().current_seq(),
            "change_history_persisted": false,
            "graph": graph,
        }))
    }

    pub(super) fn tool_reindex(&mut self) -> Result<ToolOutput> {
        let count = ProjectSession::new(
            &mut self.engine,
            &self.root,
            &self.graph_path,
            self.persist_graph,
        )
        .reindex()?;
        Ok(ToolOutput::new(json!({
            "files_indexed": count,
            "symbols_indexed": self.engine.symbol_index_count(),
            "unique_words_indexed": self.engine.word_index_count(),
            "word_indexed_files": self.engine.word_index_file_count(),
            "graph": graph_status(&self.graph_path),
            "persisted": self.persist_graph,
        })))
    }

    pub(super) fn tool_clear_index(&mut self) -> Result<ToolOutput> {
        let existed = ProjectSession::new(
            &mut self.engine,
            &self.root,
            &self.graph_path,
            self.persist_graph,
        )
        .clear_index()
        .with_context(|| format!("failed to remove graph {}", self.graph_path.display()))?;
        Ok(ToolOutput::new(json!({
            "cleared": true,
            "graph_removed": existed,
            "graph": graph_status(&self.graph_path),
        })))
    }

    pub(super) fn tool_audit(&mut self, args: &Value) -> Result<ToolOutput> {
        let max_results = opt_usize(args, "max_results")
            .or_else(|| opt_usize(args, "max"))
            .map(|value| value.min(MAX_AUDIT_RESULTS));
        let config_path = opt_str(args, "config").map(PathBuf::from);
        let config = audit::load_audit_config(
            &self.root,
            config_path.as_deref(),
            opt_bool(args, "no_config").unwrap_or(false),
        )?;
        let includes = audit_includes(args)?;
        let scope = if let Some(base) = opt_str(args, "since") {
            audit::AuditScope::GitSince {
                base: base.to_string(),
                changed_files: audit::changed_files_since(&self.root, base)?,
            }
        } else {
            audit::AuditScope::Project
        };
        let report = ProjectSession::new(
            &mut self.engine,
            &self.root,
            &self.graph_path,
            self.persist_graph,
        )
        .audit(AuditOptions {
            max_results,
            scope,
            config,
            includes,
        });
        Ok(ToolOutput::new(json!(report)))
    }

    pub(super) fn tool_pipeline(&self, args: &Value) -> Result<ToolOutput> {
        let pipeline_arg = opt_str(args, "pipeline");
        let steps_arg = args.get("steps").and_then(Value::as_array);

        let pipeline = if let Some(text) = pipeline_arg {
            text.to_string()
        } else if let Some(steps) = steps_arg {
            steps
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            bail!("pipeline requires pipeline string or steps array");
        };

        let output = crate::pipeline::run_output(&self.engine, &pipeline);
        Ok(ToolOutput::new(output.to_json(&pipeline)))
    }
}
