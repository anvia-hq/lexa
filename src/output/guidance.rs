use super::value::*;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct NextStep {
    tool: String,
    args: Value,
}

impl NextStep {
    pub(super) fn new(tool: impl Into<String>, args: Value, _reason: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args,
        }
    }
}

pub(super) fn insert_next_steps(
    map: &mut Map<String, Value>,
    steps: impl IntoIterator<Item = NextStep>,
) {
    let mut seen = BTreeSet::new();
    let rows = steps
        .into_iter()
        .filter_map(|step| {
            if step.tool.is_empty() {
                return None;
            }
            let args = if step.args.is_null() {
                "{}".to_string()
            } else {
                step.args.to_string()
            };
            if !seen.insert((step.tool.clone(), args.clone())) {
                return None;
            }
            Some(row([s(step.tool), step.args]))
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return;
    }
    map.insert("next_cols".to_string(), cols(&["tool", "args"]));
    map.insert("next".to_string(), array(rows));
}

pub(super) fn trim_summary_keywords(summary: &mut Value, limit: usize) {
    let Some(summary) = summary.as_object_mut() else {
        return;
    };
    let Some(keywords) = summary.get_mut("keywords").and_then(Value::as_array_mut) else {
        return;
    };
    let keyword_count = keywords.len();
    if keyword_count <= limit {
        return;
    }
    keywords.truncate(limit);
    summary.insert("keyword_count".to_string(), n(keyword_count));
}

pub(super) fn brief_next_steps(payload: &Value) -> Vec<NextStep> {
    let task = payload
        .get("task")
        .or_else(|| payload.get("query"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    payload
        .get("suggested_next_steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let reason = step.as_str()?.to_string();
            let lower = reason.to_ascii_lowercase();
            if lower.contains("symbol-search") {
                Some(NextStep::new(
                    "symbol_search",
                    json!({ "query": task }),
                    reason,
                ))
            } else if lower.contains("text-search") {
                Some(NextStep::new(
                    "text_search",
                    json!({ "query": task }),
                    reason,
                ))
            } else {
                Some(NextStep::new("brief", json!({ "task": task }), reason))
            }
        })
        .collect()
}

pub(super) fn should_emit_brief_next(payload: &Value, no_symbols: bool, no_snippets: bool) -> bool {
    if no_symbols && no_snippets {
        return true;
    }
    if payload
        .get("confidence")
        .and_then(Value::as_str)
        .is_some_and(|confidence| confidence.eq_ignore_ascii_case("low"))
    {
        return true;
    }
    payload
        .get("note")
        .and_then(Value::as_str)
        .is_some_and(|note| {
            let note = note.to_ascii_lowercase();
            note.contains("low-confidence") || note.contains("scope")
        })
}

#[derive(Clone)]
pub(super) struct AuditRuleGroup {
    rule: String,
    severity: String,
    count: usize,
    top_path: String,
}

pub(super) fn insert_audit_rule_rows(map: &mut Map<String, Value>, groups: Vec<AuditRuleGroup>) {
    let rows = groups
        .into_iter()
        .map(|group| {
            row([
                s(group.rule),
                s(group.severity),
                n(group.count),
                s(group.top_path),
            ])
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }
    map.insert(
        "rule_cols".to_string(),
        cols(&["rule", "severity", "count", "top_path"]),
    );
    map.insert("rules".to_string(), array(rows));
}

pub(super) fn audit_rule_groups(payload: &Value) -> Vec<AuditRuleGroup> {
    let mut indexes = BTreeMap::<String, usize>::new();
    let mut groups = Vec::<AuditRuleGroup>::new();

    for finding in payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let rule = finding
            .get("rule")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let severity = finding
            .get("severity")
            .and_then(Value::as_str)
            .unwrap_or("warning")
            .to_string();
        let path = finding
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let key = format!("{rule}\u{1f}{severity}");

        if let Some(index) = indexes.get(&key).copied() {
            groups[index].count += 1;
        } else {
            indexes.insert(key, groups.len());
            groups.push(AuditRuleGroup {
                rule,
                severity,
                count: 1,
                top_path: path,
            });
        }
    }

    groups.sort_by(|left, right| {
        audit_severity_rank(&left.severity)
            .cmp(&audit_severity_rank(&right.severity))
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.rule.cmp(&right.rule))
            .then_with(|| left.top_path.cmp(&right.top_path))
    });
    groups
}

pub(super) fn deduped_audit_rows(payload: &Value) -> Vec<Value> {
    let mut indexes = BTreeMap::<String, usize>::new();
    let mut entries = Vec::<(Vec<Value>, usize)>::new();

    for finding in payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = audit_dedupe_key(finding);
        if let Some(index) = indexes.get(&key).copied() {
            entries[index].1 += 1;
            continue;
        }

        indexes.insert(key, entries.len());
        entries.push((
            vec![
                get(finding, "severity"),
                get(finding, "rule"),
                get(finding, "path"),
                get(finding, "line_start"),
                get(finding, "title"),
            ],
            1,
        ));
    }

    entries
        .into_iter()
        .map(|(mut values, instances)| {
            values.push(n(instances));
            row(values)
        })
        .collect()
}

fn audit_dedupe_key(finding: &Value) -> String {
    [
        get(finding, "id"),
        get(finding, "path"),
        get(finding, "line_start"),
        get(finding, "title"),
    ]
    .into_iter()
    .map(|value| match value {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    })
    .collect::<Vec<_>>()
    .join("\u{1f}")
}

pub(super) fn audit_grouped_next_steps(payload: &Value) -> Vec<NextStep> {
    const MAX_PER_RULE: usize = 1;
    const MAX_TOTAL: usize = 2;

    let groups = audit_rule_groups(payload);
    let findings = payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let mut steps = Vec::new();
    let mut seen = BTreeSet::new();

    for group in groups {
        if steps.len() >= MAX_TOTAL {
            break;
        }
        let mut group_count = 0;
        let mut group_tools = BTreeSet::new();
        for finding in findings.iter().filter(|finding| {
            finding.get("rule").and_then(Value::as_str) == Some(group.rule.as_str())
                && finding.get("severity").and_then(Value::as_str) == Some(group.severity.as_str())
        }) {
            if group_count >= MAX_PER_RULE || steps.len() >= MAX_TOTAL {
                break;
            }
            for step in audit_next_steps_for_finding(finding) {
                if group_count >= MAX_PER_RULE || steps.len() >= MAX_TOTAL {
                    break;
                }
                if step.tool.is_empty()
                    || should_skip_audit_group_step(&group.rule, &group_tools, &step)
                {
                    continue;
                }
                let args = if step.args.is_null() {
                    "{}".to_string()
                } else {
                    step.args.to_string()
                };
                if !seen.insert((step.tool.clone(), args)) {
                    continue;
                }
                group_tools.insert(step.tool.clone());
                steps.push(step);
                group_count += 1;
            }
        }
    }

    steps
}

fn should_skip_audit_group_step(
    rule: &str,
    group_tools: &BTreeSet<String>,
    step: &NextStep,
) -> bool {
    rule == "dependency.hotspot" && step.tool == "outline" && group_tools.contains("outline")
}

fn audit_next_steps_for_finding(finding: &Value) -> Vec<NextStep> {
    let rule = finding.get("rule").and_then(Value::as_str).unwrap_or("");
    let path = finding.get("path").and_then(Value::as_str).unwrap_or("");
    let reason = audit_next_reason(finding);

    match rule {
        "architecture.cycle" => vec![
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "depends_on" }),
                reason.clone(),
            ),
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "imported_by" }),
                reason,
            ),
        ],
        "dependency.hotspot" => vec![
            NextStep::new("outline", json!({ "path": path }), reason.clone()),
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "imported_by" }),
                reason.clone(),
            ),
            NextStep::new(
                "trace_deps",
                json!({ "path": path, "direction": "depends_on" }),
                reason,
            ),
        ],
        "symbol.large" => audit_symbol_large_next_steps(finding, reason),
        "file.large" | "dead_code.candidate" | "dependency.unresolved_import" => {
            audit_existing_next_steps(finding, reason)
        }
        _ => audit_existing_next_steps(finding, reason),
    }
}

fn audit_symbol_large_next_steps(finding: &Value, reason: String) -> Vec<NextStep> {
    let mut steps = audit_existing_next_steps(finding, reason);
    for step in &mut steps {
        if step.tool != "read" {
            continue;
        }
        let Some(args) = step.args.as_object_mut() else {
            continue;
        };
        let line_start = args
            .get("line_start")
            .and_then(Value::as_u64)
            .or_else(|| finding.get("line_start").and_then(Value::as_u64));
        let line_end = args
            .get("line_end")
            .and_then(Value::as_u64)
            .or_else(|| finding.get("line_end").and_then(Value::as_u64));

        if let (Some(line_start), Some(line_end)) = (line_start, line_end) {
            args.insert("line_start".to_string(), json!(line_start));
            args.insert(
                "line_end".to_string(),
                json!(line_end.min(line_start.saturating_add(199))),
            );
        }
    }
    steps
}

fn audit_existing_next_steps(finding: &Value, reason: String) -> Vec<NextStep> {
    finding
        .get("next_steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            Some(NextStep::new(
                step.get("tool").and_then(Value::as_str)?,
                step.get("args").cloned().unwrap_or_else(|| json!({})),
                reason.clone(),
            ))
        })
        .collect()
}

fn audit_next_reason(finding: &Value) -> String {
    let rule = finding
        .get("rule")
        .and_then(Value::as_str)
        .unwrap_or("audit");
    let title = finding
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("finding");
    format!("{rule}: {title}")
}

fn audit_severity_rank(severity: &str) -> u8 {
    match severity {
        "high" => 0,
        "warning" => 1,
        _ => 2,
    }
}
