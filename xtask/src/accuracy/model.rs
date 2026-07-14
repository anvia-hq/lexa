use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default = "default_dataset_version")]
    pub dataset_version: String,
    #[serde(default = "default_task_count")]
    pub historical_tasks_per_repo: usize,
    pub repositories: Vec<RepositoryConfig>,
}

fn default_dataset_version() -> String {
    "v1".to_string()
}

fn default_task_count() -> usize {
    20
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryConfig {
    pub id: String,
    pub path: PathBuf,
    #[serde(rename = "ref", default = "default_ref")]
    pub reference: String,
    pub languages: Vec<String>,
    pub source_extensions: Vec<String>,
    #[serde(default)]
    pub verification_commands: Vec<Vec<String>>,
}

fn default_ref() -> String {
    "HEAD".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetMetadata {
    pub schema_version: u32,
    pub dataset_version: String,
    pub generated_at_unix_ms: u128,
    pub repositories: Vec<PinnedRepository>,
    pub historical_task_count: usize,
    pub tool_case_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedRepository {
    pub id: String,
    pub path: PathBuf,
    pub commit: String,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalTask {
    pub schema_version: u32,
    pub id: String,
    pub repo_id: String,
    pub source_commit: String,
    pub base_commit: String,
    pub query: String,
    pub relevant_paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCase {
    pub schema_version: u32,
    pub id: String,
    pub repo_id: String,
    pub commit: String,
    pub tool: String,
    pub args: Vec<String>,
    pub expected_items: Vec<String>,
    #[serde(default = "default_k")]
    pub k: usize,
    #[serde(default)]
    pub reviewed: bool,
}

fn default_k() -> usize {
    10
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditLabel {
    pub schema_version: u32,
    pub repo_id: String,
    pub commit: String,
    pub finding_id: String,
    pub rule: String,
    pub severity: String,
    pub actionability: String,
    pub path: String,
    pub line_start: Option<u32>,
    pub title: String,
    pub verdict: Option<AuditVerdict>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditVerdict {
    CorrectActionable,
    CorrectNotActionable,
    FalsePositive,
    Uncertain,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationRecipe {
    pub schema_version: u32,
    pub id: String,
    pub repo_id: String,
    pub base_commit: String,
    pub file: String,
    pub patch: MutationPatch,
    pub expectations: Vec<MutationExpectation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MutationPatch {
    Replace { before: String, after: String },
    InsertBefore { anchor: String, content: String },
    InsertAfter { anchor: String, content: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MutationExpectation {
    AuditFinding {
        rule: String,
        path: Option<String>,
    },
    ToolItems {
        tool: String,
        args: Vec<String>,
        expected_items: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub repo_id: String,
    pub category: String,
    pub tool: String,
    pub commit: String,
    pub expected_items: Vec<String>,
    pub returned_items: Vec<String>,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub reciprocal_rank: f64,
    pub latency_ms: u128,
    pub output_bytes: usize,
    pub estimated_tokens: usize,
    pub correct: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MutationResult {
    pub id: String,
    pub repo_id: String,
    pub base_commit: String,
    pub detected: bool,
    pub expectations: usize,
    pub matched_expectations: usize,
    pub latency_ms: u128,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RetrievalMetrics {
    pub cases: usize,
    pub correct_cases: usize,
    pub mean_recall_at_5: f64,
    pub mean_recall_at_10: f64,
    pub mean_reciprocal_rank: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AuditMetrics {
    pub findings: usize,
    pub reviewed: usize,
    pub unreviewed: usize,
    pub uncertain: usize,
    pub structurally_correct: usize,
    pub actionable: usize,
    pub correct_not_actionable: usize,
    pub false_positives: usize,
    pub high_severity_false_positives: usize,
    pub structural_precision: Option<f64>,
    pub actionable_precision: Option<f64>,
    pub false_discovery_rate: Option<f64>,
    pub precision_at_25: Option<f64>,
    pub label_coverage: f64,
    pub by_rule: BTreeMap<String, AuditRuleMetrics>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AuditRuleMetrics {
    pub reviewed: usize,
    pub correct_actionable: usize,
    pub correct_not_actionable: usize,
    pub false_positives: usize,
    pub uncertain: usize,
    pub structural_precision: Option<f64>,
    pub actionable_precision: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MutationMetrics {
    pub cases: usize,
    pub detected: usize,
    pub recall: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BenchmarkMetrics {
    pub schema_version: u32,
    pub dataset_version: String,
    pub run_id: String,
    pub generated_at_unix_ms: u128,
    pub lexa_version: String,
    pub lexa_git_commit: String,
    pub historical_retrieval: RetrievalMetrics,
    pub curated_tools: RetrievalMetrics,
    pub audit: AuditMetrics,
    pub mutations: MutationMetrics,
    pub total_index_ms: u128,
    pub total_query_ms: u128,
    pub total_output_bytes: usize,
    pub total_estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineDelta {
    pub metric: String,
    pub baseline: Option<f64>,
    pub current: Option<f64>,
    pub absolute_delta: Option<f64>,
}

pub fn rank_scores(expected: &[String], returned: &[String]) -> (f64, f64, f64) {
    if expected.is_empty() {
        return (1.0, 1.0, 1.0);
    }
    let expected = expected.iter().collect::<BTreeSet<_>>();
    let recall_at = |limit: usize| {
        let found = returned
            .iter()
            .take(limit)
            .filter(|item| expected.contains(item))
            .collect::<BTreeSet<_>>()
            .len();
        found as f64 / expected.len() as f64
    };
    let reciprocal_rank = returned
        .iter()
        .position(|item| expected.contains(item))
        .map(|index| 1.0 / (index + 1) as f64)
        .unwrap_or(0.0);
    (recall_at(5), recall_at(10), reciprocal_rank)
}

pub fn aggregate_retrieval(results: &[CaseResult]) -> RetrievalMetrics {
    if results.is_empty() {
        return RetrievalMetrics::default();
    }
    RetrievalMetrics {
        cases: results.len(),
        correct_cases: results.iter().filter(|result| result.correct).count(),
        mean_recall_at_5: mean(results.iter().map(|result| result.recall_at_5)),
        mean_recall_at_10: mean(results.iter().map(|result| result.recall_at_10)),
        mean_reciprocal_rank: mean(results.iter().map(|result| result.reciprocal_rank)),
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

pub fn score_audit(labels: &[AuditLabel]) -> AuditMetrics {
    let mut metrics = AuditMetrics {
        findings: labels.len(),
        ..AuditMetrics::default()
    };
    let mut top_reviewed = Vec::new();

    for label in labels {
        let Some(verdict) = label.verdict else {
            metrics.unreviewed += 1;
            continue;
        };
        metrics.reviewed += 1;
        if top_reviewed.len() < 25 && verdict != AuditVerdict::Uncertain {
            top_reviewed.push(verdict);
        }

        let rule = metrics.by_rule.entry(label.rule.clone()).or_default();
        rule.reviewed += 1;
        match verdict {
            AuditVerdict::CorrectActionable => {
                metrics.structurally_correct += 1;
                metrics.actionable += 1;
                rule.correct_actionable += 1;
            }
            AuditVerdict::CorrectNotActionable => {
                metrics.structurally_correct += 1;
                metrics.correct_not_actionable += 1;
                rule.correct_not_actionable += 1;
            }
            AuditVerdict::FalsePositive => {
                metrics.false_positives += 1;
                rule.false_positives += 1;
                if label.severity == "high" {
                    metrics.high_severity_false_positives += 1;
                }
            }
            AuditVerdict::Uncertain => {
                metrics.uncertain += 1;
                rule.uncertain += 1;
            }
        }
    }

    let structural_denominator = metrics.structurally_correct + metrics.false_positives;
    metrics.structural_precision = ratio(metrics.structurally_correct, structural_denominator);
    let useful_denominator =
        metrics.actionable + metrics.correct_not_actionable + metrics.false_positives;
    metrics.actionable_precision = ratio(metrics.actionable, useful_denominator);
    metrics.false_discovery_rate = ratio(metrics.false_positives, structural_denominator);
    metrics.precision_at_25 = if top_reviewed.is_empty() {
        None
    } else {
        Some(
            top_reviewed
                .iter()
                .filter(|verdict| **verdict == AuditVerdict::CorrectActionable)
                .count() as f64
                / top_reviewed.len() as f64,
        )
    };
    metrics.label_coverage = if metrics.findings == 0 {
        1.0
    } else {
        metrics.reviewed as f64 / metrics.findings as f64
    };

    for rule in metrics.by_rule.values_mut() {
        let structural_correct = rule.correct_actionable + rule.correct_not_actionable;
        rule.structural_precision = ratio(
            structural_correct,
            structural_correct + rule.false_positives,
        );
        rule.actionable_precision = ratio(
            rule.correct_actionable,
            rule.correct_actionable + rule.correct_not_actionable + rule.false_positives,
        );
    }

    metrics
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 / denominator as f64)
}

pub fn score_mutations(results: &[MutationResult]) -> MutationMetrics {
    let detected = results.iter().filter(|result| result.detected).count();
    MutationMetrics {
        cases: results.len(),
        detected,
        recall: ratio(detected, results.len()),
    }
}

pub fn extract_items(tool: &str, payload: &Value, path_hint: Option<&str>) -> Vec<String> {
    let mut items = Vec::new();
    match tool {
        "files" | "recent" => collect_paths(payload.get("files"), &mut items, false),
        "list" => collect_field(payload.get("entries"), "name", &mut items),
        "glob" => collect_strings(payload.get("paths"), &mut items),
        "path-search" | "path_search" | "text-search" | "text_search" | "word-refs"
        | "word_refs" | "callers" => collect_paths(
            payload.get("results"),
            &mut items,
            tool != "path-search" && tool != "path_search",
        ),
        "symbol-defs" | "symbol_defs" => {
            if let Some(results) = payload.get("results").and_then(Value::as_array) {
                for result in results {
                    let path = result
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let symbol = result.get("symbol").unwrap_or(result);
                    let name = symbol
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let line = symbol
                        .get("line_start")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    items.push(format!("{path}:{line}:{name}"));
                }
            }
        }
        "symbol-search" | "symbol_search" => {
            if let Some(results) = payload.get("results").and_then(Value::as_array) {
                for result in results {
                    let path = result
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let name = result
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let line = result
                        .get("line_start")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    items.push(format!("{path}:{line}:{name}"));
                }
            }
        }
        "outline" => {
            let path = path_hint.unwrap_or_default();
            if let Some(symbols) = payload.get("symbols").and_then(Value::as_array) {
                for symbol in symbols {
                    let name = symbol
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let line = symbol
                        .get("line_start")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    items.push(format!("{path}:{line}:{name}"));
                }
            }
        }
        "trace-deps" | "trace_deps" => collect_strings(payload.get("dependencies"), &mut items),
        "brief" => {
            collect_paths(payload.get("relevant_symbols"), &mut items, false);
            collect_paths(payload.get("snippets"), &mut items, false);
        }
        _ => {}
    }
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(item.clone()));
    items
}

fn collect_strings(value: Option<&Value>, items: &mut Vec<String>) {
    if let Some(values) = value.and_then(Value::as_array) {
        items.extend(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string),
        );
    }
}

fn collect_field(value: Option<&Value>, field: &str, items: &mut Vec<String>) {
    if let Some(values) = value.and_then(Value::as_array) {
        items.extend(
            values
                .iter()
                .filter_map(|item| item.get(field).and_then(Value::as_str))
                .map(ToString::to_string),
        );
    }
}

fn collect_paths(value: Option<&Value>, items: &mut Vec<String>, include_line: bool) {
    if let Some(values) = value.and_then(Value::as_array) {
        for item in values {
            let Some(path) = item.get("path").and_then(Value::as_str) else {
                continue;
            };
            if include_line {
                let line = item
                    .get("line_num")
                    .or_else(|| item.get("line_start"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                items.push(format!("{path}:{line}"));
            } else {
                items.push(path.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_scores_deduplicate_expected_and_returned_items() {
        let expected = vec!["a".to_string(), "b".to_string()];
        let returned = vec!["x".to_string(), "a".to_string(), "a".to_string()];

        let (recall_5, recall_10, reciprocal_rank) = rank_scores(&expected, &returned);

        assert_eq!(recall_5, 0.5);
        assert_eq!(recall_10, 0.5);
        assert_eq!(reciprocal_rank, 0.5);
    }

    #[test]
    fn audit_scoring_separates_correct_but_not_actionable() {
        let labels = vec![
            audit_label(AuditVerdict::CorrectActionable, "high"),
            audit_label(AuditVerdict::CorrectNotActionable, "warning"),
            audit_label(AuditVerdict::FalsePositive, "high"),
            audit_label(AuditVerdict::Uncertain, "warning"),
        ];

        let metrics = score_audit(&labels);

        assert_eq!(metrics.structural_precision, Some(2.0 / 3.0));
        assert_eq!(metrics.actionable_precision, Some(1.0 / 3.0));
        assert_eq!(metrics.high_severity_false_positives, 1);
        assert_eq!(metrics.uncertain, 1);
    }

    fn audit_label(verdict: AuditVerdict, severity: &str) -> AuditLabel {
        AuditLabel {
            schema_version: SCHEMA_VERSION,
            repo_id: "repo".to_string(),
            commit: "abc".to_string(),
            finding_id: format!("finding-{severity}"),
            rule: "rule".to_string(),
            severity: severity.to_string(),
            actionability: "actionable".to_string(),
            path: "src/lib.rs".to_string(),
            line_start: Some(1),
            title: "title".to_string(),
            verdict: Some(verdict),
            notes: String::new(),
        }
    }
}
