use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::audit;
use crate::edit::{self, EditOp};

pub(super) fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    opt_str(args, key).with_context(|| format!("missing required string: {key}"))
}

pub(super) fn req_any_str<'a>(args: &'a Value, keys: &[&str]) -> Result<&'a str> {
    keys.iter()
        .find_map(|key| opt_str(args, key))
        .with_context(|| format!("missing required string: {}", keys.join("|")))
}

pub(super) fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub(super) fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

pub(super) fn opt_u32(args: &Value, key: &str) -> Option<u32> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

pub(super) fn opt_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

pub(super) fn opt_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
}

pub(super) fn audit_includes(args: &Value) -> Result<audit::AuditIncludes> {
    let mut includes = audit::AuditIncludes::default();
    let Some(values) = args.get("include").and_then(Value::as_array) else {
        return Ok(includes);
    };

    for value in values {
        match value.as_str() {
            Some("dead-code") => includes.dead_code = true,
            Some(other) => bail!("unknown audit include: {other}"),
            None => bail!("audit include values must be strings"),
        }
    }

    Ok(includes)
}

pub(super) fn parse_edit_op(op: &str) -> Result<EditOp> {
    match op {
        "replace" => Ok(EditOp::Replace),
        "insert" => Ok(EditOp::Insert),
        "delete" => Ok(EditOp::Delete),
        _ => bail!("op must be replace, insert, or delete"),
    }
}

pub(super) fn parse_anchor_placement(placement: &str) -> Result<edit::AnchorPlacement> {
    match placement {
        "before" => Ok(edit::AnchorPlacement::Before),
        "after" => Ok(edit::AnchorPlacement::After),
        _ => bail!("placement must be before or after"),
    }
}

pub(super) fn parse_preview_mode(mode: &str) -> Result<edit::PreviewMode> {
    match mode {
        "compact" => Ok(edit::PreviewMode::Compact),
        "full" => Ok(edit::PreviewMode::Full),
        _ => bail!("preview_mode must be compact or full"),
    }
}

pub(super) fn edit_op_label(
    op: Option<EditOp>,
    replace_text: Option<&str>,
    anchor: Option<&str>,
) -> &'static str {
    if replace_text.is_some() {
        "replace-text"
    } else if anchor.is_some() {
        "anchor"
    } else if let Some(op) = op {
        match op {
            EditOp::Replace => "replace",
            EditOp::Insert => "insert",
            EditOp::Delete => "delete",
        }
    } else {
        "unknown"
    }
}

pub(super) fn preview_mode_str(mode: edit::PreviewMode) -> &'static str {
    match mode {
        edit::PreviewMode::Compact => "compact",
        edit::PreviewMode::Full => "full",
    }
}
