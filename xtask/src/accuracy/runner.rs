use super::git::{apply_mutation, with_worktree};
use super::model::{
    extract_items, rank_scores, AuditLabel, CaseResult, Manifest, MutationExpectation,
    MutationRecipe, MutationResult, RepositoryConfig, ToolCase, SCHEMA_VERSION,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[derive(Debug)]
pub struct LexaOutput {
    pub payload: Value,
    pub elapsed_ms: u128,
    pub bytes: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Default)]
pub struct RunArtifacts {
    pub historical: Vec<CaseResult>,
    pub curated: Vec<CaseResult>,
    pub audit_labels: Vec<AuditLabel>,
    pub mutations: Vec<MutationResult>,
    pub total_index_ms: u128,
    pub total_query_ms: u128,
    pub total_output_bytes: usize,
    pub total_estimated_tokens: usize,
}

pub fn lexa_version(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .with_context(|| format!("failed to run {} --version", binary.display()))?;
    if !output.status.success() {
        bail!(
            "{} --version failed: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn collect_audit_labels(
    binary: &Path,
    repo: &RepositoryConfig,
    commit: &str,
    existing: &[AuditLabel],
) -> Result<Vec<AuditLabel>> {
    let existing = existing
        .iter()
        .map(|label| (label_key(label), label))
        .collect::<BTreeMap<_, _>>();
    with_worktree(&repo.path, commit, |worktree| {
        index(binary, worktree)?;
        let output = audit(binary, worktree)?;
        audit_labels_from_payload(&repo.id, commit, &output.payload, &existing)
    })
}

pub fn run_all(
    binary: &Path,
    manifest: &Manifest,
    historical_tasks: &[super::model::HistoricalTask],
    tool_cases: &[ToolCase],
    labels: &[AuditLabel],
    mutations: &[MutationRecipe],
) -> Result<RunArtifacts> {
    let repos = manifest
        .repositories
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect::<BTreeMap<_, _>>();
    let mut artifacts = RunArtifacts::default();

    for (task_index, task) in historical_tasks.iter().enumerate() {
        println!(
            "  history {}/{}: {}",
            task_index + 1,
            historical_tasks.len(),
            task.id
        );
        let repo = repos
            .get(task.repo_id.as_str())
            .with_context(|| format!("task {} references unknown repo", task.id))?;
        let case = with_worktree(&repo.path, &task.base_commit, |worktree| {
            let index_ms = index(binary, worktree)?;
            let output = run_lexa_json(
                binary,
                worktree,
                &[
                    "brief".to_string(),
                    task.query.clone(),
                    "--max-results".to_string(),
                    "10".to_string(),
                ],
            )?;
            Ok((index_ms, historical_result(task, &output)))
        })?;
        artifacts.total_index_ms += case.0;
        add_case_totals(&mut artifacts, &case.1);
        artifacts.historical.push(case.1);
    }

    let grouped_cases = group_tool_cases(tool_cases);
    for ((repo_id, commit), cases) in grouped_cases {
        println!("  curated: {repo_id} at {}", short_commit(&commit));
        let repo = repos
            .get(repo_id.as_str())
            .with_context(|| format!("tool cases reference unknown repo '{repo_id}'"))?;
        let (index_ms, results) = with_worktree(&repo.path, &commit, |worktree| {
            let index_ms = index(binary, worktree)?;
            let mut results = Vec::new();
            for case in &cases {
                results.push(run_tool_case(binary, worktree, case));
            }
            Ok((index_ms, results))
        })?;
        artifacts.total_index_ms += index_ms;
        for result in results {
            add_case_totals(&mut artifacts, &result);
            artifacts.curated.push(result);
        }
    }

    for repo in &manifest.repositories {
        let commit = super::git::resolve_commit(&repo.path, &repo.reference)?;
        println!("  audit: {} at {}", repo.id, short_commit(&commit));
        let repo_labels = labels
            .iter()
            .filter(|label| label.repo_id == repo.id && label.commit == commit)
            .map(|label| (label_key(label), label))
            .collect::<BTreeMap<_, _>>();
        let (index_ms, output, current_labels) = with_worktree(&repo.path, &commit, |worktree| {
            let index_ms = index(binary, worktree)?;
            let output = audit(binary, worktree)?;
            let current =
                audit_labels_from_payload(&repo.id, &commit, &output.payload, &repo_labels)?;
            Ok((index_ms, output, current))
        })?;
        artifacts.total_index_ms += index_ms;
        artifacts.total_query_ms += output.elapsed_ms;
        artifacts.total_output_bytes += output.bytes;
        artifacts.total_estimated_tokens += output.estimated_tokens;
        artifacts.audit_labels.extend(current_labels);
    }

    for (mutation_index, mutation) in mutations.iter().enumerate() {
        println!(
            "  mutation {}/{}: {}",
            mutation_index + 1,
            mutations.len(),
            mutation.id
        );
        let repo = repos
            .get(mutation.repo_id.as_str())
            .with_context(|| format!("mutation {} references unknown repo", mutation.id))?;
        let started = Instant::now();
        let result = with_worktree(&repo.path, &mutation.base_commit, |worktree| {
            apply_mutation(worktree, &mutation.file, &mutation.patch)?;
            index(binary, worktree)?;
            evaluate_mutation(binary, worktree, mutation)
        });
        let elapsed_ms = started.elapsed().as_millis();
        artifacts.mutations.push(match result {
            Ok((matched, expectations)) => MutationResult {
                id: mutation.id.clone(),
                repo_id: mutation.repo_id.clone(),
                base_commit: mutation.base_commit.clone(),
                detected: matched == expectations,
                expectations,
                matched_expectations: matched,
                latency_ms: elapsed_ms,
                error: None,
            },
            Err(error) => MutationResult {
                id: mutation.id.clone(),
                repo_id: mutation.repo_id.clone(),
                base_commit: mutation.base_commit.clone(),
                detected: false,
                expectations: mutation.expectations.len(),
                matched_expectations: 0,
                latency_ms: elapsed_ms,
                error: Some(format!("{error:#}")),
            },
        });
    }

    artifacts
        .historical
        .sort_by(|left, right| left.id.cmp(&right.id));
    artifacts
        .curated
        .sort_by(|left, right| left.id.cmp(&right.id));
    artifacts.audit_labels.sort_by_key(label_key);
    artifacts
        .mutations
        .sort_by(|left, right| left.id.cmp(&right.id));
    Ok(artifacts)
}

fn add_case_totals(artifacts: &mut RunArtifacts, result: &CaseResult) {
    artifacts.total_query_ms += result.latency_ms;
    artifacts.total_output_bytes += result.output_bytes;
    artifacts.total_estimated_tokens += result.estimated_tokens;
}

fn group_tool_cases(cases: &[ToolCase]) -> BTreeMap<(String, String), Vec<ToolCase>> {
    let mut groups = BTreeMap::new();
    for case in cases.iter().filter(|case| case.reviewed) {
        groups
            .entry((case.repo_id.clone(), case.commit.clone()))
            .or_insert_with(Vec::new)
            .push(case.clone());
    }
    groups
}

fn historical_result(task: &super::model::HistoricalTask, output: &LexaOutput) -> CaseResult {
    let returned = extract_items("brief", &output.payload, None);
    let (recall_at_5, recall_at_10, reciprocal_rank) = rank_scores(&task.relevant_paths, &returned);
    CaseResult {
        id: task.id.clone(),
        repo_id: task.repo_id.clone(),
        category: "historical_retrieval".to_string(),
        tool: "brief".to_string(),
        commit: task.base_commit.clone(),
        expected_items: task.relevant_paths.clone(),
        returned_items: returned,
        recall_at_5,
        recall_at_10,
        reciprocal_rank,
        latency_ms: output.elapsed_ms,
        output_bytes: output.bytes,
        estimated_tokens: output.estimated_tokens,
        correct: recall_at_10 == 1.0,
        error: None,
    }
}

fn run_tool_case(binary: &Path, worktree: &Path, case: &ToolCase) -> CaseResult {
    match run_lexa_json(binary, worktree, &tool_args(case)) {
        Ok(output) => {
            let path_hint = (case.tool == "outline")
                .then(|| case.args.first())
                .flatten();
            let returned =
                extract_items(&case.tool, &output.payload, path_hint.map(String::as_str));
            let (recall_at_5, recall_at_10, reciprocal_rank) =
                rank_scores(&case.expected_items, &returned);
            let returned_set = returned.iter().collect::<BTreeSet<_>>();
            let correct = case
                .expected_items
                .iter()
                .all(|item| returned_set.contains(item));
            CaseResult {
                id: case.id.clone(),
                repo_id: case.repo_id.clone(),
                category: "curated_tool".to_string(),
                tool: case.tool.clone(),
                commit: case.commit.clone(),
                expected_items: case.expected_items.clone(),
                returned_items: returned,
                recall_at_5,
                recall_at_10,
                reciprocal_rank,
                latency_ms: output.elapsed_ms,
                output_bytes: output.bytes,
                estimated_tokens: output.estimated_tokens,
                correct,
                error: None,
            }
        }
        Err(error) => CaseResult {
            id: case.id.clone(),
            repo_id: case.repo_id.clone(),
            category: "curated_tool".to_string(),
            tool: case.tool.clone(),
            commit: case.commit.clone(),
            expected_items: case.expected_items.clone(),
            returned_items: Vec::new(),
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            reciprocal_rank: 0.0,
            latency_ms: 0,
            output_bytes: 0,
            estimated_tokens: 0,
            correct: false,
            error: Some(format!("{error:#}")),
        },
    }
}

fn tool_args(case: &ToolCase) -> Vec<String> {
    let mut args = Vec::with_capacity(case.args.len() + 1);
    args.push(case.tool.clone());
    args.extend(case.args.clone());
    args
}

fn evaluate_mutation(
    binary: &Path,
    worktree: &Path,
    mutation: &MutationRecipe,
) -> Result<(usize, usize)> {
    let needs_audit = mutation
        .expectations
        .iter()
        .any(|expectation| matches!(expectation, MutationExpectation::AuditFinding { .. }));
    let audit_payload = if needs_audit {
        Some(audit(binary, worktree)?.payload)
    } else {
        None
    };
    let mut matched = 0;
    for expectation in &mutation.expectations {
        let is_match = match expectation {
            MutationExpectation::AuditFinding { rule, path } => audit_payload
                .as_ref()
                .and_then(|payload| payload.get("findings"))
                .and_then(Value::as_array)
                .is_some_and(|findings| {
                    findings.iter().any(|finding| {
                        finding.get("rule").and_then(Value::as_str) == Some(rule)
                            && path.as_ref().is_none_or(|path| {
                                finding.get("path").and_then(Value::as_str) == Some(path)
                            })
                    })
                }),
            MutationExpectation::ToolItems {
                tool,
                args,
                expected_items,
            } => {
                let mut command_args = vec![tool.clone()];
                command_args.extend(args.clone());
                let output = run_lexa_json(binary, worktree, &command_args)?;
                let path_hint = (tool == "outline").then(|| args.first()).flatten();
                let returned = extract_items(tool, &output.payload, path_hint.map(String::as_str));
                let returned = returned.iter().collect::<BTreeSet<_>>();
                expected_items.iter().all(|item| returned.contains(item))
            }
        };
        if is_match {
            matched += 1;
        }
    }
    Ok((matched, mutation.expectations.len()))
}

fn index(binary: &Path, worktree: &Path) -> Result<u128> {
    let output = run_lexa_json(binary, worktree, &["index".to_string(), ".".to_string()])?;
    Ok(output.elapsed_ms)
}

fn audit(binary: &Path, worktree: &Path) -> Result<LexaOutput> {
    run_lexa_json(
        binary,
        worktree,
        &[
            "audit".to_string(),
            "--max".to_string(),
            "1000".to_string(),
            "--include".to_string(),
            "dead-code".to_string(),
            "--no-config".to_string(),
        ],
    )
}

fn run_lexa_json(binary: &Path, worktree: &Path, args: &[String]) -> Result<LexaOutput> {
    let started = Instant::now();
    let output = Command::new(binary)
        .args(args)
        .env("LEXA_INTERNAL_BENCHMARK_JSON", "1")
        .current_dir(worktree)
        .output()
        .with_context(|| format!("failed to run {} {:?}", binary.display(), args))?;
    let elapsed_ms = started.elapsed().as_millis();
    if !output.status.success() {
        bail!(
            "{} {:?} failed in {}\nstdout:\n{}\nstderr:\n{}",
            binary.display(),
            args,
            worktree.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let bytes = output.stdout.len();
    let payload = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "failed to decode JSON from {} {:?}: {}",
            binary.display(),
            args,
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    Ok(LexaOutput {
        payload,
        elapsed_ms,
        bytes,
        estimated_tokens: estimated_tokens(bytes),
    })
}

fn estimated_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

fn audit_labels_from_payload(
    repo_id: &str,
    commit: &str,
    payload: &Value,
    existing: &BTreeMap<String, &AuditLabel>,
) -> Result<Vec<AuditLabel>> {
    let findings = payload
        .get("findings")
        .and_then(Value::as_array)
        .context("audit JSON is missing findings array")?;
    let mut labels = Vec::with_capacity(findings.len());
    for finding in findings {
        let finding_id = required_string(finding, "id")?;
        let mut label = AuditLabel {
            schema_version: SCHEMA_VERSION,
            repo_id: repo_id.to_string(),
            commit: commit.to_string(),
            finding_id,
            rule: required_string(finding, "rule")?,
            severity: required_string(finding, "severity")?,
            actionability: required_string(finding, "actionability")?,
            path: required_string(finding, "path")?,
            line_start: finding
                .get("line_start")
                .and_then(Value::as_u64)
                .map(|line| line as u32),
            title: required_string(finding, "title")?,
            verdict: None,
            notes: String::new(),
        };
        if let Some(previous) = existing.get(&label_key(&label)) {
            label.verdict = previous.verdict;
            label.notes.clone_from(&previous.notes);
        }
        labels.push(label);
    }
    Ok(labels)
}

fn required_string(value: &Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .with_context(|| format!("audit finding is missing string field '{field}'"))
}

pub fn label_key(label: &AuditLabel) -> String {
    format!("{}|{}|{}", label.repo_id, label.commit, label.finding_id)
}

fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
