use anyhow::{anyhow, bail, Context, Result};
use clap::ValueEnum;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::engine::hash_content;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EditOp {
    Replace,
    Insert,
    Delete,
}

pub struct EditRequest {
    pub path: PathBuf,
    pub op: EditOp,
    pub range_start: Option<u32>,
    pub range_end: Option<u32>,
    pub after: Option<u32>,
    pub content: Option<String>,
    pub if_hash: Option<String>,
    pub dry_run: bool,
}

pub struct EditResult {
    pub new_content: String,
    pub old_hash: u64,
    pub new_hash: u64,
    pub line_count: usize,
    pub changed: bool,
    pub preview: String,
}

pub struct CreateRequest {
    pub path: PathBuf,
    pub content: String,
    pub overwrite: bool,
    pub dry_run: bool,
}

pub struct CreateResult {
    pub hash: u64,
    pub line_count: usize,
    pub byte_size: u64,
    pub changed: bool,
}

pub fn apply_edit(req: &EditRequest) -> Result<EditResult> {
    let path = req.path.as_path();
    let old_content = std::fs::read_to_string(&req.path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let old_hash = hash_content(&old_content);

    if let Some(expected) = &req.if_hash {
        if !expected.eq_ignore_ascii_case(&format!("{old_hash:x}")) {
            bail!(
                "hash mismatch for {}: expected {}, actual {:x}",
                path.display(),
                expected,
                old_hash
            );
        }
    }

    let new_content = build_new_content(&old_content, req)?;
    let new_hash = hash_content(&new_content);
    let changed = old_hash != new_hash;
    let preview = build_preview(&old_content, &new_content);

    if changed && !req.dry_run {
        atomic_write(path, &new_content)?;
    }

    Ok(EditResult {
        line_count: new_content.lines().count().max(1),
        new_content,
        old_hash,
        new_hash,
        changed,
        preview,
    })
}

pub fn create_file(req: &CreateRequest) -> Result<CreateResult> {
    if req.path.exists() && !req.overwrite {
        bail!("file already exists: {}", req.path.display());
    }

    let hash = hash_content(&req.content);
    let line_count = req.content.lines().count().max(1);
    let byte_size = req.content.len() as u64;

    if !req.dry_run {
        atomic_write(&req.path, &req.content)?;
    }

    Ok(CreateResult {
        hash,
        line_count,
        byte_size,
        changed: !req.dry_run,
    })
}

fn build_new_content(old_content: &str, req: &EditRequest) -> Result<String> {
    let mut lines: Vec<String> = old_content.lines().map(ToString::to_string).collect();
    let had_trailing_newline = old_content.ends_with('\n');

    match req.op {
        EditOp::Replace => {
            let (start, end) = concrete_range(req)?;
            ensure_range_in_bounds(start, end, lines.len())?;
            let replacement = replacement_lines(req)?;
            lines.splice(start..end, replacement);
        }
        EditOp::Insert => {
            let replacement = replacement_lines(req)?;
            let after = req.after.unwrap_or(0) as usize;
            let insert_at = after.min(lines.len());
            lines.splice(insert_at..insert_at, replacement);
        }
        EditOp::Delete => {
            let (start, end) = concrete_range(req)?;
            ensure_range_in_bounds(start, end, lines.len())?;
            lines.drain(start..end);
        }
    }

    let mut result = lines.join("\n");
    if had_trailing_newline && !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

fn ensure_range_in_bounds(start: usize, end: usize, len: usize) -> Result<()> {
    if start >= len || end > len {
        bail!(
            "line range is outside file: requested {}-{}, file has {} lines",
            start + 1,
            end,
            len
        );
    }
    Ok(())
}

fn concrete_range(req: &EditRequest) -> Result<(usize, usize)> {
    let start = req
        .range_start
        .ok_or_else(|| anyhow!("replace/delete requires --line-range START-END"))?;
    let end = req.range_end.unwrap_or(start);
    if start == 0 || end < start {
        bail!("invalid line range: {start}-{end}");
    }
    Ok(((start - 1) as usize, end as usize))
}

fn replacement_lines(req: &EditRequest) -> Result<Vec<String>> {
    let content = req
        .content
        .as_deref()
        .ok_or_else(|| anyhow!("replace/insert requires --content or --content-file"))?;
    Ok(content.lines().map(ToString::to_string).collect())
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let tmp_path = temp_path(parent, filename, content);

    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to create {}", tmp_path.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("failed to write {}", tmp_path.display()))?;
        file.sync_all().ok();
    }

    std::fs::rename(&tmp_path, path).with_context(|| {
        let _ = std::fs::remove_file(&tmp_path);
        format!("failed to replace {}", path.display())
    })?;
    Ok(())
}

fn temp_path(parent: &Path, filename: &str, content: &str) -> PathBuf {
    parent.join(format!(
        ".{filename}.lexa-edit-{}-{:x}.tmp",
        std::process::id(),
        hash_content(content)
    ))
}

fn build_preview(old_content: &str, new_content: &str) -> String {
    if old_content == new_content {
        return "unchanged".to_string();
    }

    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let mut out = String::new();
    out.push_str("--- before\n+++ after\n");

    let max = old_lines.len().max(new_lines.len());
    for idx in 0..max {
        match (old_lines.get(idx), new_lines.get(idx)) {
            (Some(old), Some(new)) if old == new => {}
            (Some(old), Some(new)) => {
                out.push_str(&format!("-{:>5}: {old}\n", idx + 1));
                out.push_str(&format!("+{:>5}: {new}\n", idx + 1));
            }
            (Some(old), None) => out.push_str(&format!("-{:>5}: {old}\n", idx + 1)),
            (None, Some(new)) => out.push_str(&format!("+{:>5}: {new}\n", idx + 1)),
            (None, None) => {}
        }
    }

    out
}
