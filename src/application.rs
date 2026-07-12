use anyhow::Result;
use std::path::Path;

use crate::edit::{self, AnchorPlacement, EditOp, PreviewMode};
use crate::engine::{Engine, ReadFileResult};
use crate::project_path::{normalize_project_path, project_target_path, PathMode};
use crate::snapshot;
use crate::store;
use crate::{audit, audit::AuditOptions};

pub struct ProjectSession<'a> {
    engine: &'a mut Engine,
    root: &'a Path,
    graph_path: &'a Path,
    persist_graph: bool,
}

pub struct ReadRequest<'a> {
    pub path: &'a str,
    pub existing_only: bool,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub compact: bool,
    pub if_hash: Option<&'a str>,
}

pub struct ReadResult {
    pub path: String,
    pub file: Option<ReadFileResult>,
}

pub struct PatchRequest<'a> {
    pub path: &'a str,
    pub op: Option<EditOp>,
    pub range_start: Option<u32>,
    pub range_end: Option<u32>,
    pub after: Option<u32>,
    pub content: Option<String>,
    pub replace_text: Option<String>,
    pub anchor: Option<String>,
    pub placement: Option<AnchorPlacement>,
    pub preview_mode: PreviewMode,
    pub if_hash: Option<String>,
    pub dry_run: bool,
}

pub struct PatchResult {
    pub path: String,
    pub edit: edit::EditResult,
}

pub struct CreateRequest<'a> {
    pub path: &'a str,
    pub content: String,
    pub overwrite: bool,
    pub dry_run: bool,
}

pub struct CreateResult {
    pub path: String,
    pub create: edit::CreateResult,
    pub would_create: bool,
}

impl<'a> ProjectSession<'a> {
    pub fn new(
        engine: &'a mut Engine,
        root: &'a Path,
        graph_path: &'a Path,
        persist_graph: bool,
    ) -> Self {
        Self {
            engine,
            root,
            graph_path,
            persist_graph,
        }
    }

    pub fn read(&self, request: ReadRequest<'_>) -> Result<ReadResult> {
        let mode = if request.existing_only {
            PathMode::Existing
        } else {
            PathMode::Create
        };
        let path = normalize_project_path(self.root, request.path, mode)?;
        let file = self.engine.read_file_rich(
            &path,
            request.line_start,
            request.line_end,
            request.compact,
            request.if_hash,
        );
        Ok(ReadResult { path, file })
    }

    pub fn patch(&mut self, request: PatchRequest<'_>) -> Result<PatchResult> {
        let path = normalize_project_path(self.root, request.path, PathMode::Existing)?;
        let effective_op = effective_edit_op(
            request.op,
            request.replace_text.as_deref(),
            request.anchor.as_deref(),
        )?;
        let edit_request = edit::EditRequest {
            path: project_target_path(self.root, &path),
            op: request.op,
            range_start: request.range_start,
            range_end: request.range_end,
            after: request.after,
            content: request.content,
            replace_text: request.replace_text,
            anchor: request.anchor,
            placement: request.placement,
            preview_mode: request.preview_mode,
            if_hash: request.if_hash,
            dry_run: request.dry_run,
        };
        let edit = edit::apply_edit(&edit_request)?;

        if edit.changed && !edit_request.dry_run {
            self.engine
                .index_edited_file(&path, &edit.new_content, store_op(effective_op));
            self.persist()?;
        }

        Ok(PatchResult { path, edit })
    }

    pub fn create(&mut self, request: CreateRequest<'_>) -> Result<CreateResult> {
        let path = normalize_project_path(self.root, request.path, PathMode::Create)?;
        let target = project_target_path(self.root, &path);
        let would_create = request.dry_run && !target.exists();
        let create_request = edit::CreateRequest {
            path: target,
            content: request.content.clone(),
            overwrite: request.overwrite,
            dry_run: request.dry_run,
        };
        let create = edit::create_file(&create_request)?;

        if !request.dry_run {
            self.engine
                .index_edited_file(&path, &request.content, store::Op::Create);
            self.persist()?;
        }

        Ok(CreateResult {
            path,
            create,
            would_create,
        })
    }

    pub fn audit(&self, options: AuditOptions) -> audit::AuditReport {
        audit::run_audit(self.engine, options)
    }

    pub fn reindex(&mut self) -> Result<usize> {
        let mut engine = Engine::new(16_384);
        let count = engine.index_project(self.root);
        if self.persist_graph {
            snapshot::write_snapshot(&engine, self.graph_path)?;
        }
        *self.engine = engine;
        Ok(count)
    }

    pub fn clear_index(&mut self) -> Result<bool> {
        let existed = self.graph_path.exists();
        if existed {
            std::fs::remove_file(self.graph_path)?;
        }
        *self.engine = Engine::new(16_384);
        Ok(existed)
    }

    fn persist(&self) -> Result<()> {
        if self.persist_graph {
            snapshot::write_snapshot(self.engine, self.graph_path)?;
        }
        Ok(())
    }
}

fn effective_edit_op(
    op: Option<EditOp>,
    replace_text: Option<&str>,
    anchor: Option<&str>,
) -> Result<EditOp> {
    match (op, replace_text.is_some(), anchor.is_some()) {
        (Some(op), false, false) => Ok(op),
        (None, true, false) => Ok(EditOp::Replace),
        (None, false, true) => Ok(EditOp::Insert),
        _ => anyhow::bail!("patch requires exactly one target: operation, replace_text, or anchor"),
    }
}

fn store_op(op: EditOp) -> store::Op {
    match op {
        EditOp::Replace => store::Op::Replace,
        EditOp::Insert => store::Op::Insert,
        EditOp::Delete => store::Op::Delete,
    }
}
