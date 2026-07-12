use super::args::*;
use super::response::ToolOutput;
use super::server::McpServer;
use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::application::{self, ProjectSession};
use crate::edit;

impl McpServer {
    pub(super) fn tool_read(&mut self, args: &Value) -> Result<ToolOutput> {
        let line_start = opt_u32(args, "line_start");
        let line_end = opt_u32(args, "line_end");
        let compact = opt_bool(args, "compact").unwrap_or(false);
        let operation = ProjectSession::new(
            &mut self.engine,
            &self.root,
            &self.graph_path,
            self.persist_graph,
        )
        .read(application::ReadRequest {
            path: req_str(args, "path")?,
            existing_only: true,
            line_start,
            line_end,
            compact,
            if_hash: opt_str(args, "if_hash"),
        })?;
        let path = operation.path;
        let result = operation
            .file
            .with_context(|| format!("file not found: {path}"))?;

        if result.unchanged {
            let hash = format!("{:x}", result.hash);
            return Ok(ToolOutput::new(json!({
                "path": path,
                "hash": hash,
                "unchanged": true,
                "line_start": line_start,
                "line_end": line_end,
                "content": ""
            })));
        }

        let hash = format!("{:x}", result.hash);
        Ok(ToolOutput::new(json!({
            "path": path,
            "hash": hash,
            "unchanged": false,
            "line_start": line_start,
            "line_end": line_end,
            "compact": compact,
            "content": result.content,
        })))
    }

    pub(super) fn tool_patch(&mut self, args: &Value) -> Result<ToolOutput> {
        let op = opt_str(args, "op").map(parse_edit_op).transpose()?;
        let replace_text = opt_str(args, "replace_text").map(ToString::to_string);
        let anchor = opt_str(args, "anchor").map(ToString::to_string);
        let placement = opt_str(args, "placement")
            .map(parse_anchor_placement)
            .transpose()?;
        let preview_mode = opt_str(args, "preview_mode")
            .map(parse_preview_mode)
            .transpose()?
            .unwrap_or(edit::PreviewMode::Compact);

        let dry_run = opt_bool(args, "dry_run").unwrap_or(false);
        let operation = ProjectSession::new(
            &mut self.engine,
            &self.root,
            &self.graph_path,
            self.persist_graph,
        )
        .patch(application::PatchRequest {
            path: req_str(args, "path")?,
            op,
            range_start: opt_u32(args, "range_start"),
            range_end: opt_u32(args, "range_end"),
            after: opt_u32(args, "after"),
            content: opt_str(args, "content").map(ToString::to_string),
            replace_text,
            anchor,
            placement,
            preview_mode,
            if_hash: opt_str(args, "if_hash").map(ToString::to_string),
            dry_run,
        })?;
        let rel_path = operation.path;
        let result = operation.edit;
        let op_label = edit_op_label(op, opt_str(args, "replace_text"), opt_str(args, "anchor"));
        if dry_run {
            return Ok(ToolOutput::new(json!({
                "path": rel_path,
                "op": op_label,
                "dry_run": true,
                "changed": result.changed,
                "old_hash": format!("{:x}", result.old_hash),
                "new_hash": format!("{:x}", result.new_hash),
                "line_count": result.line_count,
                "lines_added": result.lines_added,
                "lines_removed": result.lines_removed,
                "preview_mode": preview_mode_str(preview_mode),
                "preview": result.preview,
            })));
        }

        if result.changed {
            let hash = format!("{:x}", result.new_hash);
            Ok(ToolOutput::new(json!({
                "path": rel_path,
                "op": op_label,
                "dry_run": false,
                "changed": true,
                "hash": hash,
                "line_count": result.line_count,
                "lines_added": result.lines_added,
                "lines_removed": result.lines_removed,
                "graph": self.graph_path.display().to_string(),
                "change_sequence": self.engine.store().current_seq(),
            })))
        } else {
            let hash = format!("{:x}", result.new_hash);
            Ok(ToolOutput::new(json!({
                "path": rel_path,
                "op": op_label,
                "dry_run": false,
                "changed": false,
                "hash": hash,
                "line_count": result.line_count,
                "lines_added": result.lines_added,
                "lines_removed": result.lines_removed,
            })))
        }
    }

    pub(super) fn tool_create(&mut self, args: &Value) -> Result<ToolOutput> {
        let content = opt_str(args, "content").unwrap_or("").to_string();
        let overwrite = opt_bool(args, "overwrite").unwrap_or(false);
        let dry_run = opt_bool(args, "dry_run").unwrap_or(false);

        let operation = ProjectSession::new(
            &mut self.engine,
            &self.root,
            &self.graph_path,
            self.persist_graph,
        )
        .create(application::CreateRequest {
            path: req_str(args, "path")?,
            content,
            overwrite,
            dry_run,
        })?;
        let rel_path = operation.path;
        let result = operation.create;
        let would_create = operation.would_create;

        let hash = format!("{:x}", result.hash);
        let mut payload = json!({
                "path": rel_path,
                "op": "create",
                "dry_run": dry_run,
                "changed": result.changed,
                "hash": hash,
                "line_count": result.line_count,
                "byte_size": result.byte_size,
                "change_sequence": self.engine.store().current_seq(),
        });
        if would_create {
            payload["would_create"] = json!(true);
        }
        Ok(ToolOutput::new(payload))
    }
}
