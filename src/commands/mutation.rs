use crate::cli::Cli;
use anyhow::{bail, Result};
use lexa::application::{self, ProjectSession};
use lexa::edit;
use lexa::engine;
use lexa::project_path::project_target_path;
use serde_json::json;
use std::path::{Path, PathBuf};

use super::shared::*;

pub(crate) fn cmd_read(
    path: &str,
    line_start: Option<u32>,
    line_end: Option<u32>,
    compact: bool,
    if_hash: Option<&str>,
    show_hash: bool,
    cli: &Cli,
) -> Result<()> {
    let mut engine = load_engine(cli)?;
    let root = std::env::current_dir()?;
    let operation = ProjectSession::new(&mut engine, &root, Path::new(""), false).read(
        application::ReadRequest {
            path,
            existing_only: false,
            line_start,
            line_end,
            compact,
            if_hash,
        },
    )?;
    let path = operation.path;
    if engine.file_count() == 0 && project_target_path(&root, &path).exists() {
        bail!("no files indexed; run 'lexa index .' before reading files");
    }

    match operation.file {
        Some(result) => {
            if cli.json {
                return print_agent_result(json!({
                    "path": path,
                    "hash": format!("{:x}", result.hash),
                    "unchanged": result.unchanged,
                    "line_start": line_start,
                    "line_end": line_end,
                    "compact": compact,
                    "content": result.content,
                }));
            }
            if result.unchanged {
                println!("unchanged:{:x}", result.hash);
                return Ok(());
            }
            if show_hash || if_hash.is_some() {
                println!("hash:{:x}", result.hash);
            }
            print!("{}", result.content);
        }
        None => {
            if cli.json {
                return print_agent_result(json!({"error": "file_not_found", "path": path}));
            }
            println!("File not found: {}", path);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_edit(
    path: &str,
    op: Option<edit::EditOp>,
    line_range: Option<&str>,
    after: Option<u32>,
    replace_text: Option<&str>,
    anchor: Option<&str>,
    placement: Option<edit::AnchorPlacement>,
    preview_mode: edit::PreviewMode,
    content: Option<&str>,
    content_file: Option<&PathBuf>,
    if_hash: Option<&str>,
    dry_run: bool,
    cli: &Cli,
) -> Result<()> {
    let root = current_root()?;
    let (range_start, range_end) = if let Some(range) = line_range {
        parse_line_range(range)?
    } else {
        (None, None)
    };

    let edit_content = if let Some(path) = content_file {
        Some(std::fs::read_to_string(path)?)
    } else {
        content.map(ToString::to_string)
    };

    let (mut engine, snap_path) = if !dry_run && !cli.no_graph {
        load_existing_engine_for_root(&root, cli)?
    } else {
        (engine::Engine::new(16), graph_path_for_root(&root, cli))
    };
    let operation = ProjectSession::new(&mut engine, &root, &snap_path, !cli.no_graph).patch(
        application::PatchRequest {
            path,
            op,
            range_start,
            range_end,
            after,
            content: edit_content,
            replace_text: replace_text.map(ToString::to_string),
            anchor: anchor.map(ToString::to_string),
            placement,
            preview_mode,
            if_hash: if_hash.map(ToString::to_string),
            dry_run,
        },
    )?;
    let rel_path = operation.path;
    let result = operation.edit;
    let op_label = edit_op_label(op, replace_text, anchor);

    if dry_run {
        if cli.json {
            return print_agent_result(json!({
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
            }));
        }
        println!("{}", result.preview);
        println!("old_hash:{:x}", result.old_hash);
        println!("new_hash:{:x}", result.new_hash);
        return Ok(());
    }

    if result.changed {
        if cli.json {
            return print_agent_result(json!({
                "path": rel_path,
                "op": op_label,
                "dry_run": false,
                "changed": true,
                "hash": format!("{:x}", result.new_hash),
                "line_count": result.line_count,
                "lines_added": result.lines_added,
                "lines_removed": result.lines_removed,
                "graph": (!cli.no_graph).then(|| snap_path.display().to_string()),
                "persisted": !cli.no_graph,
                "change_sequence": engine.store().current_seq(),
            }));
        }
        println!(
            "{}",
            format_edit_applied(&rel_path, &result, result.new_hash)
        );
        if !cli.no_graph {
            println!("Graph saved to {}", snap_path.display());
        }
    } else {
        if cli.json {
            return print_agent_result(json!({
                "path": rel_path,
                "op": op_label,
                "dry_run": false,
                "changed": false,
                "hash": format!("{:x}", result.new_hash),
                "line_count": result.line_count,
                "lines_added": result.lines_added,
                "lines_removed": result.lines_removed,
            }));
        }
        println!("edit unchanged: hash:{:x}", result.new_hash);
    }

    Ok(())
}

pub(crate) fn cmd_create(
    path: &str,
    content: Option<&str>,
    content_file: Option<&PathBuf>,
    overwrite: bool,
    dry_run: bool,
    cli: &Cli,
) -> Result<()> {
    let root = current_root()?;
    let content = if let Some(path) = content_file {
        std::fs::read_to_string(path)?
    } else {
        content.unwrap_or("").to_string()
    };

    let (mut engine, snap_path) = if !dry_run && !cli.no_graph {
        load_existing_engine_for_root(&root, cli)?
    } else {
        (engine::Engine::new(16), graph_path_for_root(&root, cli))
    };
    let operation = ProjectSession::new(&mut engine, &root, &snap_path, !cli.no_graph).create(
        application::CreateRequest {
            path,
            content,
            overwrite,
            dry_run,
        },
    )?;
    let rel_path = operation.path;
    let result = operation.create;
    let would_create = operation.would_create;

    if cli.json {
        let mut payload = json!({
            "path": rel_path,
            "op": "create",
            "dry_run": dry_run,
            "changed": result.changed,
            "hash": format!("{:x}", result.hash),
            "line_count": result.line_count,
            "byte_size": result.byte_size,
        });
        if would_create {
            payload["would_create"] = json!(true);
        }
        return print_agent_result(payload);
    }

    if dry_run {
        println!(
            "create dry-run: {} lines, hash:{:x}",
            result.line_count, result.hash
        );
    } else {
        println!(
            "file created: {} lines, hash:{:x}",
            result.line_count, result.hash
        );
    }

    Ok(())
}

pub(crate) fn format_edit_applied(path: &str, result: &edit::EditResult, hash: u64) -> String {
    if result.lines_added == 0 && result.lines_removed == 0 {
        return format!(
            "edit applied to {path}: content changed without line-count change ({} total), hash:{hash:x}",
            result.line_count
        );
    }

    format!(
        "edit applied to {path}: +{} -{} lines ({} total), hash:{hash:x}",
        result.lines_added, result.lines_removed, result.line_count
    )
}

pub(crate) fn edit_op_label(
    op: Option<edit::EditOp>,
    replace_text: Option<&str>,
    anchor: Option<&str>,
) -> &'static str {
    if replace_text.is_some() {
        "replace-text"
    } else if anchor.is_some() {
        "anchor"
    } else if let Some(op) = op {
        edit_op_str(op)
    } else {
        "unknown"
    }
}
