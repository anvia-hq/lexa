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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PreviewMode {
    Compact,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AnchorPlacement {
    Before,
    After,
}

pub struct EditRequest {
    pub path: PathBuf,
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

pub struct EditResult {
    pub new_content: String,
    pub old_hash: u64,
    pub new_hash: u64,
    pub line_count: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
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
    let (lines_added, lines_removed) = diff_stats(&old_content, &new_content);
    let preview = build_preview(&old_content, &new_content, req.preview_mode);

    if changed && !req.dry_run {
        atomic_write(path, &new_content)?;
    }

    Ok(EditResult {
        line_count: new_content.lines().count().max(1),
        new_content,
        old_hash,
        new_hash,
        changed,
        lines_added,
        lines_removed,
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
    validate_target_shape(req)?;

    if let Some(replace_text) = &req.replace_text {
        let replacement = req
            .content
            .as_deref()
            .ok_or_else(|| anyhow!("--replace-text requires --content or --content-file"))?;
        return replace_exact_text(old_content, replace_text, replacement);
    }

    if let Some(anchor) = &req.anchor {
        let content = req
            .content
            .as_deref()
            .ok_or_else(|| anyhow!("--anchor requires --content or --content-file"))?;
        let placement = req
            .placement
            .ok_or_else(|| anyhow!("--anchor requires --placement before|after"))?;
        return insert_at_anchor(old_content, anchor, placement, content);
    }

    let mut lines: Vec<String> = old_content.lines().map(ToString::to_string).collect();
    let had_trailing_newline = old_content.ends_with('\n');

    match req
        .op
        .ok_or_else(|| anyhow!("patch requires an operation or --replace-text/--anchor"))?
    {
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

fn validate_target_shape(req: &EditRequest) -> Result<()> {
    let mut target_count = 0;
    if req.op.is_some() {
        target_count += 1;
    }
    if req.replace_text.is_some() {
        target_count += 1;
    }
    if req.anchor.is_some() {
        target_count += 1;
    }
    if target_count != 1 {
        bail!("patch requires exactly one target: operation, --replace-text, or --anchor");
    }

    if req.replace_text.is_some()
        && (req.range_start.is_some()
            || req.range_end.is_some()
            || req.after.is_some()
            || req.placement.is_some())
    {
        bail!("--replace-text cannot be combined with line range, --after, or --placement");
    }

    if req.anchor.is_some()
        && (req.range_start.is_some() || req.range_end.is_some() || req.after.is_some())
    {
        bail!("--anchor cannot be combined with line range or --after");
    }

    Ok(())
}

fn replace_exact_text(old_content: &str, old_text: &str, replacement: &str) -> Result<String> {
    if old_text.is_empty() {
        bail!("--replace-text cannot be empty");
    }
    let (start, end) = unique_match(old_content, old_text, "--replace-text")?;
    let mut result = String::with_capacity(old_content.len() - old_text.len() + replacement.len());
    result.push_str(&old_content[..start]);
    result.push_str(replacement);
    result.push_str(&old_content[end..]);
    Ok(result)
}

fn insert_at_anchor(
    old_content: &str,
    anchor: &str,
    placement: AnchorPlacement,
    content: &str,
) -> Result<String> {
    if anchor.is_empty() {
        bail!("--anchor cannot be empty");
    }
    let (start, end) = unique_match(old_content, anchor, "--anchor")?;
    let insert_at = match placement {
        AnchorPlacement::Before => start,
        AnchorPlacement::After => end,
    };

    let mut result = String::with_capacity(old_content.len() + content.len() + 2);
    result.push_str(&old_content[..insert_at]);
    if placement == AnchorPlacement::After
        && !content.is_empty()
        && !old_content[..insert_at].ends_with('\n')
    {
        result.push('\n');
    }
    result.push_str(content);
    if !content.is_empty()
        && !content.ends_with('\n')
        && !old_content[insert_at..].starts_with('\n')
    {
        result.push('\n');
    }
    result.push_str(&old_content[insert_at..]);
    Ok(result)
}

fn unique_match(haystack: &str, needle: &str, flag: &str) -> Result<(usize, usize)> {
    let mut matches = haystack.match_indices(needle);
    let Some((start, _)) = matches.next() else {
        bail!(
            "{flag} did not match the file content. If the text contains shell metacharacters, wrap it in single quotes or pass replacement content with --content-file."
        );
    };
    if matches.next().is_some() {
        bail!("{flag} matched multiple locations; provide a unique exact text");
    }
    Ok((start, start + needle.len()))
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

fn build_preview(old_content: &str, new_content: &str, mode: PreviewMode) -> String {
    if old_content == new_content {
        return "unchanged".to_string();
    }

    if mode == PreviewMode::Full {
        return build_full_preview(old_content, new_content);
    }

    build_compact_preview(old_content, new_content)
}

fn build_full_preview(old_content: &str, new_content: &str) -> String {
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

fn build_compact_preview(old_content: &str, new_content: &str) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let mut out = String::new();
    out.push_str("--- before\n+++ after\n");

    let bounds = diff_bounds(&old_lines, &new_lines);
    let context = 3usize;
    let old_context_start = bounds.old_start.saturating_sub(context);
    let old_context_end = (bounds.old_end + context).min(old_lines.len());
    let new_context_end = (bounds.new_end + context).min(new_lines.len());

    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        display_line(bounds.old_start),
        bounds.old_end.saturating_sub(bounds.old_start),
        display_line(bounds.new_start),
        bounds.new_end.saturating_sub(bounds.new_start)
    ));

    for (idx, line) in old_lines
        .iter()
        .enumerate()
        .take(bounds.old_start)
        .skip(old_context_start)
    {
        out.push_str(&format!(" {:>5}: {line}\n", idx + 1));
    }
    for (idx, line) in old_lines
        .iter()
        .enumerate()
        .take(bounds.old_end)
        .skip(bounds.old_start)
    {
        out.push_str(&format!("-{:>5}: {line}\n", idx + 1));
    }
    for (idx, line) in new_lines
        .iter()
        .enumerate()
        .take(bounds.new_end)
        .skip(bounds.new_start)
    {
        out.push_str(&format!("+{:>5}: {line}\n", idx + 1));
    }
    for (idx, line) in old_lines
        .iter()
        .enumerate()
        .take(old_context_end)
        .skip(bounds.old_end)
    {
        out.push_str(&format!(" {:>5}: {line}\n", idx + 1));
    }

    if old_context_end < old_lines.len() || new_context_end < new_lines.len() {
        out.push_str(" ...\n");
    }

    out
}

fn display_line(zero_based: usize) -> usize {
    zero_based.saturating_add(1)
}

struct DiffBounds {
    old_start: usize,
    old_end: usize,
    new_start: usize,
    new_end: usize,
}

fn diff_bounds(old_lines: &[&str], new_lines: &[&str]) -> DiffBounds {
    let mut start = 0usize;
    while start < old_lines.len() && start < new_lines.len() && old_lines[start] == new_lines[start]
    {
        start += 1;
    }

    let mut suffix = 0usize;
    while suffix < old_lines.len().saturating_sub(start)
        && suffix < new_lines.len().saturating_sub(start)
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    DiffBounds {
        old_start: start,
        old_end: old_lines.len() - suffix,
        new_start: start,
        new_end: new_lines.len() - suffix,
    }
}

fn diff_stats(old_content: &str, new_content: &str) -> (usize, usize) {
    if old_content == new_content {
        return (0, 0);
    }

    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let bounds = diff_bounds(&old_lines, &new_lines);
    (
        bounds.new_end.saturating_sub(bounds.new_start),
        bounds.old_end.saturating_sub(bounds.old_start),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn request(path: PathBuf, op: EditOp) -> EditRequest {
        EditRequest {
            path,
            op: Some(op),
            range_start: None,
            range_end: None,
            after: None,
            content: None,
            replace_text: None,
            anchor: None,
            placement: None,
            preview_mode: PreviewMode::Compact,
            if_hash: None,
            dry_run: false,
        }
    }

    fn edit_err(req: &EditRequest) -> String {
        match apply_edit(req) {
            Ok(_) => panic!("expected edit error"),
            Err(err) => err.to_string(),
        }
    }

    fn create_err(req: &CreateRequest) -> String {
        match create_file(req) {
            Ok(_) => panic!("expected create error"),
            Err(err) => err.to_string(),
        }
    }

    #[test]
    fn replace_updates_requested_line_and_preserves_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\nthree\n");
        let mut req = request(path.clone(), EditOp::Replace);
        req.range_start = Some(2);
        req.range_end = Some(2);
        req.content = Some("TWO".to_string());

        let result = apply_edit(&req).unwrap();

        assert!(result.changed);
        assert_eq!(result.new_content, "one\nTWO\nthree\n");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "one\nTWO\nthree\n");
        assert!(result.preview.contains("-    2: two"));
        assert!(result.preview.contains("+    2: TWO"));
    }

    #[test]
    fn insert_supports_start_middle_and_after_end_positions() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "notes.txt", "b\nc\n");

        let mut start = request(path.clone(), EditOp::Insert);
        start.after = Some(0);
        start.content = Some("a".to_string());
        assert_eq!(apply_edit(&start).unwrap().new_content, "a\nb\nc\n");

        let mut middle = request(path.clone(), EditOp::Insert);
        middle.after = Some(2);
        middle.content = Some("between".to_string());
        assert_eq!(
            apply_edit(&middle).unwrap().new_content,
            "a\nb\nbetween\nc\n"
        );

        let mut end = request(path.clone(), EditOp::Insert);
        end.after = Some(99);
        end.content = Some("z".to_string());
        assert_eq!(
            apply_edit(&end).unwrap().new_content,
            "a\nb\nbetween\nc\nz\n"
        );
    }

    #[test]
    fn delete_removes_line_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\nthree\nfour\n");
        let mut req = request(path.clone(), EditOp::Delete);
        req.range_start = Some(2);
        req.range_end = Some(3);

        let result = apply_edit(&req).unwrap();

        assert_eq!(result.new_content, "one\nfour\n");
        assert_eq!(result.line_count, 2);
    }

    #[test]
    fn dry_run_reports_change_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\n");
        let mut req = request(path.clone(), EditOp::Replace);
        req.range_start = Some(1);
        req.content = Some("ONE".to_string());
        req.dry_run = true;

        let result = apply_edit(&req).unwrap();

        assert!(result.changed);
        assert_eq!(result.new_content, "ONE\ntwo\n");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn unchanged_edit_keeps_file_and_reports_unchanged_preview() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "same\n");
        let mut req = request(path.clone(), EditOp::Replace);
        req.range_start = Some(1);
        req.content = Some("same".to_string());

        let result = apply_edit(&req).unwrap();

        assert!(!result.changed);
        assert_eq!(result.preview, "unchanged");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "same\n");
    }

    #[test]
    fn compact_preview_does_not_report_shifted_insertions() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\nthree\nfour\nfive\n");
        let mut req = request(path, EditOp::Insert);
        req.after = Some(2);
        req.content = Some("inserted".to_string());
        req.dry_run = true;

        let result = apply_edit(&req).unwrap();

        assert_eq!(result.lines_added, 1);
        assert_eq!(result.lines_removed, 0);
        assert!(result.preview.contains("+    3: inserted"));
        assert!(!result.preview.contains("-    3: three"));
        assert!(!result.preview.contains("-    4: four"));
    }

    #[test]
    fn full_preview_keeps_legacy_shifted_line_comparison() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\nthree\n");
        let mut req = request(path, EditOp::Insert);
        req.after = Some(1);
        req.content = Some("inserted".to_string());
        req.preview_mode = PreviewMode::Full;
        req.dry_run = true;

        let result = apply_edit(&req).unwrap();

        assert!(result.preview.contains("-    2: two"));
        assert!(result.preview.contains("+    2: inserted"));
    }

    #[test]
    fn replace_text_requires_unique_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\nthree\n");
        let mut req = request(path.clone(), EditOp::Replace);
        req.op = None;
        req.replace_text = Some("two".to_string());
        req.content = Some("TWO".to_string());

        let result = apply_edit(&req).unwrap();

        assert_eq!(result.new_content, "one\nTWO\nthree\n");
        assert_eq!(result.lines_added, 1);
        assert_eq!(result.lines_removed, 1);

        let mut missing = req;
        missing.replace_text = Some("absent".to_string());
        assert!(edit_err(&missing).contains("did not match"));
        assert!(edit_err(&missing).contains("single quotes"));

        std::fs::write(&path, "same\nsame\n").unwrap();
        let mut ambiguous = request(path, EditOp::Replace);
        ambiguous.op = None;
        ambiguous.replace_text = Some("same".to_string());
        ambiguous.content = Some("changed".to_string());
        assert!(edit_err(&ambiguous).contains("matched multiple locations"));
    }

    #[test]
    fn anchor_inserts_before_or_after_unique_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\nthree\n");
        let mut after = request(path.clone(), EditOp::Insert);
        after.op = None;
        after.anchor = Some("two".to_string());
        after.placement = Some(AnchorPlacement::After);
        after.content = Some("after".to_string());

        let result = apply_edit(&after).unwrap();
        assert_eq!(result.new_content, "one\ntwo\nafter\nthree\n");
        assert_eq!(result.lines_added, 1);
        assert_eq!(result.lines_removed, 0);

        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let mut before = request(path, EditOp::Insert);
        before.op = None;
        before.anchor = Some("two".to_string());
        before.placement = Some(AnchorPlacement::Before);
        before.content = Some("before".to_string());

        let result = apply_edit(&before).unwrap();
        assert_eq!(result.new_content, "one\nbefore\ntwo\nthree\n");
    }

    #[test]
    fn edit_validates_hash_range_and_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "app.rs", "one\ntwo\n");

        let mut bad_hash = request(path.clone(), EditOp::Delete);
        bad_hash.range_start = Some(1);
        bad_hash.if_hash = Some("deadbeef".to_string());
        assert!(edit_err(&bad_hash).contains("hash mismatch"));

        let missing_range = request(path.clone(), EditOp::Delete);
        assert_eq!(
            edit_err(&missing_range),
            "replace/delete requires --line-range START-END"
        );

        let mut invalid_range = request(path.clone(), EditOp::Replace);
        invalid_range.range_start = Some(3);
        invalid_range.content = Some("three".to_string());
        assert!(edit_err(&invalid_range).contains("line range is outside file"));

        let mut missing_content = request(path, EditOp::Insert);
        missing_content.after = Some(1);
        assert_eq!(
            edit_err(&missing_content),
            "replace/insert requires --content or --content-file"
        );
    }

    #[test]
    fn create_file_handles_dry_run_overwrite_and_line_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let dry_run = CreateRequest {
            path: path.clone(),
            content: "one\ntwo\n".to_string(),
            overwrite: false,
            dry_run: true,
        };

        let result = create_file(&dry_run).unwrap();

        assert!(!result.changed);
        assert_eq!(result.line_count, 2);
        assert_eq!(result.byte_size, 8);
        assert!(!path.exists());

        let create = CreateRequest {
            dry_run: false,
            ..dry_run
        };
        let result = create_file(&create).unwrap();
        assert!(result.changed);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");

        let err = create_err(&CreateRequest {
            path: path.clone(),
            content: "blocked".to_string(),
            overwrite: false,
            dry_run: false,
        });
        assert!(err.contains("file already exists"));

        create_file(&CreateRequest {
            path: path.clone(),
            content: "overwritten".to_string(),
            overwrite: true,
            dry_run: false,
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "overwritten");
    }
}
