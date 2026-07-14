mod git;
mod model;
mod runner;

use anyhow::{bail, Context, Result};
use model::{
    aggregate_retrieval, score_audit, score_mutations, AuditLabel, BaselineDelta, BenchmarkMetrics,
    DatasetMetadata, HistoricalTask, Manifest, MutationRecipe, PinnedRepository, ToolCase,
    SCHEMA_VERSION,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_CONFIG: &str = ".lexa-bench/manifest.json";

pub fn dispatch(args: Vec<String>) -> Result<()> {
    let Some((subcommand, rest)) = args.split_first() else {
        bail!("usage: xtask accuracy-bench <prepare|run> [--config PATH] [--lexa-bin PATH]");
    };
    match subcommand.as_str() {
        "prepare" => {
            let flags = Flags::parse(rest, false)?;
            prepare(&flags)
        }
        "run" => {
            let flags = Flags::parse(rest, true)?;
            run(&flags)
        }
        other => bail!("unknown accuracy-bench subcommand '{other}'"),
    }
}

#[derive(Debug)]
struct Flags {
    config: PathBuf,
    lexa_bin: Option<PathBuf>,
    baseline: Option<PathBuf>,
    history_limit: Option<usize>,
}

impl Flags {
    fn parse(args: &[String], allow_baseline: bool) -> Result<Self> {
        let mut config = PathBuf::from(DEFAULT_CONFIG);
        let mut lexa_bin = None;
        let mut baseline = None;
        let mut history_limit = None;
        let mut index = 0;
        while index < args.len() {
            let flag = &args[index];
            let value = args
                .get(index + 1)
                .with_context(|| format!("missing value for accuracy benchmark flag '{flag}'"))?;
            match flag.as_str() {
                "--config" => config = PathBuf::from(value),
                "--lexa-bin" => lexa_bin = Some(PathBuf::from(value)),
                "--baseline" if allow_baseline => baseline = Some(PathBuf::from(value)),
                "--history-limit" if allow_baseline => {
                    history_limit = Some(
                        value
                            .parse()
                            .with_context(|| format!("invalid --history-limit value '{value}'"))?,
                    )
                }
                _ => bail!("unknown accuracy benchmark flag '{flag}'"),
            }
            index += 2;
        }
        Ok(Self {
            config,
            lexa_bin,
            baseline,
            history_limit,
        })
    }
}

fn prepare(flags: &Flags) -> Result<()> {
    let repo_root = super::find_repo_root()?;
    let (manifest, data_dir) = load_manifest(&flags.config)?;
    let binary = resolve_binary(&repo_root, flags.lexa_bin.as_deref())?;
    let labels_path = data_dir.join("audit-labels.jsonl");
    let existing_labels = read_jsonl_if_exists::<AuditLabel>(&labels_path)?;
    let tool_cases_path = data_dir.join("tool-cases.jsonl");
    let existing_cases = read_jsonl_if_exists::<ToolCase>(&tool_cases_path)?;
    let mut pinned = Vec::new();
    let mut tasks = Vec::new();
    let mut cases = Vec::new();
    let mut labels = Vec::new();

    println!(
        "preparing accuracy dataset '{}' from {} repositories",
        manifest.dataset_version,
        manifest.repositories.len()
    );
    for repo in &manifest.repositories {
        let commit = git::resolve_commit(&repo.path, &repo.reference)?;
        println!("  {} at {}", repo.id, short_commit(&commit));
        pinned.push(PinnedRepository {
            id: repo.id.clone(),
            path: repo.path.clone(),
            commit: commit.clone(),
            languages: repo.languages.clone(),
        });
        tasks.extend(git::discover_historical_tasks(
            repo,
            &commit,
            manifest.historical_tasks_per_repo,
        )?);
        cases.extend(git::generate_automatic_tool_cases(repo, &commit)?);
        labels.extend(runner::collect_audit_labels(
            &binary,
            repo,
            &commit,
            &existing_labels,
        )?);
    }

    let generated_case_ids = cases
        .iter()
        .map(|case| case.id.clone())
        .collect::<BTreeSet<_>>();
    cases.extend(
        existing_cases
            .into_iter()
            .filter(|case| case.reviewed && !generated_case_ids.contains(&case.id)),
    );
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    let metadata = DatasetMetadata {
        schema_version: SCHEMA_VERSION,
        dataset_version: manifest.dataset_version.clone(),
        generated_at_unix_ms: unix_ms(),
        repositories: pinned,
        historical_task_count: tasks.len(),
        tool_case_count: cases.len(),
    };
    write_json(&data_dir.join("dataset.json"), &metadata)?;
    write_jsonl(&data_dir.join("historical-tasks.jsonl"), &tasks)?;
    write_jsonl(&tool_cases_path, &cases)?;
    write_jsonl(&labels_path, &labels)?;
    let mutations = data_dir.join("mutations.jsonl");
    if !mutations.exists() {
        write_jsonl::<MutationRecipe>(&mutations, &[])?;
    }

    println!(
        "prepared {} historical tasks, {} tool cases, and {} audit findings",
        tasks.len(),
        cases.len(),
        labels.len()
    );
    println!(
        "review labels in {} and define mutations in {}",
        labels_path.display(),
        mutations.display()
    );
    Ok(())
}

fn run(flags: &Flags) -> Result<()> {
    let repo_root = super::find_repo_root()?;
    let (manifest, data_dir) = load_manifest(&flags.config)?;
    let binary = resolve_binary(&repo_root, flags.lexa_bin.as_deref())?;
    let metadata: DatasetMetadata = read_json(&data_dir.join("dataset.json"))?;
    validate_dataset(&manifest, &metadata)?;
    let mut tasks = read_jsonl::<HistoricalTask>(&data_dir.join("historical-tasks.jsonl"))?;
    if let Some(limit) = flags.history_limit {
        tasks.truncate(limit);
    }
    let cases = read_jsonl::<ToolCase>(&data_dir.join("tool-cases.jsonl"))?;
    let labels = read_jsonl::<AuditLabel>(&data_dir.join("audit-labels.jsonl"))?;
    let mutations = read_jsonl::<MutationRecipe>(&data_dir.join("mutations.jsonl"))?;

    println!(
        "running Lexa accuracy benchmark: {} history, {} tool, {} audit labels, {} mutations",
        tasks.len(),
        cases.iter().filter(|case| case.reviewed).count(),
        labels.len(),
        mutations.len()
    );
    let artifacts = runner::run_all(&binary, &manifest, &tasks, &cases, &labels, &mutations)?;
    let lexa_version = runner::lexa_version(&binary)?;
    let lexa_git_commit = git::resolve_commit(&repo_root, "HEAD")?;
    let run_id = format!("{}-{}", unix_ms(), short_commit(&lexa_git_commit));
    let metrics = BenchmarkMetrics {
        schema_version: SCHEMA_VERSION,
        dataset_version: manifest.dataset_version.clone(),
        run_id: run_id.clone(),
        generated_at_unix_ms: unix_ms(),
        lexa_version,
        lexa_git_commit,
        historical_retrieval: aggregate_retrieval(&artifacts.historical),
        curated_tools: aggregate_retrieval(&artifacts.curated),
        audit: score_audit(&artifacts.audit_labels),
        mutations: score_mutations(&artifacts.mutations),
        total_index_ms: artifacts.total_index_ms,
        total_query_ms: artifacts.total_query_ms,
        total_output_bytes: artifacts.total_output_bytes,
        total_estimated_tokens: artifacts.total_estimated_tokens,
    };

    let run_dir = data_dir.join("runs").join(&run_id);
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create {}", run_dir.display()))?;
    let mut all_cases = artifacts.historical.clone();
    all_cases.extend(artifacts.curated.clone());
    write_jsonl(&run_dir.join("cases.jsonl"), &all_cases)?;
    write_jsonl(
        &run_dir.join("audit-findings.jsonl"),
        &artifacts.audit_labels,
    )?;
    write_jsonl(
        &run_dir.join("mutation-results.jsonl"),
        &artifacts.mutations,
    )?;
    write_json(&run_dir.join("metrics.json"), &metrics)?;

    let deltas = if let Some(baseline_path) = &flags.baseline {
        let baseline: BenchmarkMetrics = read_json(baseline_path)?;
        baseline_deltas(&baseline, &metrics)
    } else {
        Vec::new()
    };
    fs::write(run_dir.join("summary.md"), render_summary(&metrics))?;
    fs::write(run_dir.join("regressions.md"), render_deltas(&deltas))?;

    println!("accuracy report: {}", run_dir.join("summary.md").display());
    println!(
        "history R@10 {:.1}%, audit structural precision {}, mutation recall {}",
        metrics.historical_retrieval.mean_recall_at_10 * 100.0,
        format_optional_percent(metrics.audit.structural_precision),
        format_optional_percent(metrics.mutations.recall)
    );
    if metrics.audit.unreviewed > 0 {
        println!(
            "note: {} current audit findings remain unreviewed",
            metrics.audit.unreviewed
        );
    }
    Ok(())
}

fn load_manifest(path: &Path) -> Result<(Manifest, PathBuf)> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve benchmark config {}", path.display()))?;
    let data_dir = canonical
        .parent()
        .context("benchmark config has no parent directory")?
        .to_path_buf();
    let mut manifest: Manifest = read_json(&canonical)?;
    if manifest.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported manifest schema version {}; expected {SCHEMA_VERSION}",
            manifest.schema_version
        );
    }
    if manifest.repositories.is_empty() {
        bail!("benchmark manifest must contain at least one repository");
    }
    let mut ids = BTreeSet::new();
    for repo in &mut manifest.repositories {
        if !ids.insert(repo.id.clone()) {
            bail!("duplicate benchmark repository id '{}'", repo.id);
        }
        if repo.source_extensions.is_empty() {
            bail!("repository '{}' has no source_extensions", repo.id);
        }
        if repo.path.is_relative() {
            repo.path = data_dir.join(&repo.path);
        }
        repo.path = repo.path.canonicalize().with_context(|| {
            format!(
                "failed to resolve repository '{}': {}",
                repo.id,
                repo.path.display()
            )
        })?;
        if !repo.path.join(".git").exists() && !is_git_worktree(&repo.path) {
            bail!("repository '{}' is not a Git worktree", repo.id);
        }
    }
    Ok((manifest, data_dir))
}

fn is_git_worktree(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn validate_dataset(manifest: &Manifest, metadata: &DatasetMetadata) -> Result<()> {
    if metadata.schema_version != SCHEMA_VERSION {
        bail!("dataset schema version does not match the runner");
    }
    if metadata.dataset_version != manifest.dataset_version {
        bail!(
            "dataset version '{}' does not match manifest version '{}'; run prepare",
            metadata.dataset_version,
            manifest.dataset_version
        );
    }
    let configured = manifest
        .repositories
        .iter()
        .map(|repo| repo.id.as_str())
        .collect::<BTreeSet<_>>();
    let pinned = metadata
        .repositories
        .iter()
        .map(|repo| repo.id.as_str())
        .collect::<BTreeSet<_>>();
    if configured != pinned {
        bail!("dataset repositories do not match the manifest; run prepare");
    }
    Ok(())
}

fn resolve_binary(repo_root: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(explicit) = explicit {
        return explicit
            .canonicalize()
            .with_context(|| format!("failed to resolve Lexa binary {}", explicit.display()));
    }
    let binary = repo_root.join("target/release/lexa");
    if !binary.is_file() {
        println!("building release Lexa binary");
        let status = Command::new("cargo")
            .args(["build", "--release", "--locked", "--bin", "lexa"])
            .current_dir(repo_root)
            .status()
            .context("failed to build release Lexa binary")?;
        if !status.success() {
            bail!("failed to build release Lexa binary");
        }
    }
    binary
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", binary.display()))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let encoded = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&encoded).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    ensure_parent(path)?;
    let encoded = serde_json::to_vec_pretty(value)?;
    fs::write(path, encoded).with_context(|| format!("failed to write {}", path.display()))
}

fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut values = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(
            serde_json::from_str(&line).with_context(|| {
                format!("failed to parse {} line {}", path.display(), index + 1)
            })?,
        );
    }
    Ok(values)
}

fn read_jsonl_if_exists<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    if path.exists() {
        read_jsonl(path)
    } else {
        Ok(Vec::new())
    }
}

fn write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> Result<()> {
    ensure_parent(path)?;
    let file =
        fs::File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, value)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn baseline_deltas(baseline: &BenchmarkMetrics, current: &BenchmarkMetrics) -> Vec<BaselineDelta> {
    [
        (
            "historical_recall_at_10",
            Some(baseline.historical_retrieval.mean_recall_at_10),
            Some(current.historical_retrieval.mean_recall_at_10),
        ),
        (
            "curated_recall_at_10",
            Some(baseline.curated_tools.mean_recall_at_10),
            Some(current.curated_tools.mean_recall_at_10),
        ),
        (
            "audit_structural_precision",
            baseline.audit.structural_precision,
            current.audit.structural_precision,
        ),
        (
            "audit_actionable_precision",
            baseline.audit.actionable_precision,
            current.audit.actionable_precision,
        ),
        (
            "mutation_recall",
            baseline.mutations.recall,
            current.mutations.recall,
        ),
    ]
    .into_iter()
    .map(|(metric, baseline, current)| BaselineDelta {
        metric: metric.to_string(),
        baseline,
        current,
        absolute_delta: baseline.zip(current).map(|(old, new)| new - old),
    })
    .collect()
}

fn render_summary(metrics: &BenchmarkMetrics) -> String {
    let mut out = String::new();
    writeln!(out, "# Lexa Accuracy Benchmark\n").expect("write string");
    writeln!(out, "- Dataset: `{}`", metrics.dataset_version).expect("write string");
    writeln!(out, "- Lexa: `{}`", metrics.lexa_version).expect("write string");
    writeln!(out, "- Commit: `{}`\n", metrics.lexa_git_commit).expect("write string");
    writeln!(out, "## Retrieval\n").expect("write string");
    writeln!(out, "| Suite | Cases | Correct | R@5 | R@10 | MRR |").expect("write string");
    writeln!(out, "| --- | ---: | ---: | ---: | ---: | ---: |").expect("write string");
    render_retrieval_row(
        &mut out,
        "Historical commits",
        &metrics.historical_retrieval,
    );
    render_retrieval_row(&mut out, "Curated tools", &metrics.curated_tools);

    writeln!(out, "\n## Audit\n").expect("write string");
    writeln!(out, "- Findings: {}", metrics.audit.findings).expect("write string");
    writeln!(
        out,
        "- Label coverage: {:.1}%",
        metrics.audit.label_coverage * 100.0
    )
    .expect("write string");
    writeln!(
        out,
        "- Structural precision: {}",
        format_optional_percent(metrics.audit.structural_precision)
    )
    .expect("write string");
    writeln!(
        out,
        "- Actionable precision: {}",
        format_optional_percent(metrics.audit.actionable_precision)
    )
    .expect("write string");
    if metrics.audit.reviewed == 0 {
        writeln!(out, "- False positives: n/a (no findings reviewed)").expect("write string");
    } else {
        writeln!(
            out,
            "- False positives: {} ({} high severity)",
            metrics.audit.false_positives, metrics.audit.high_severity_false_positives
        )
        .expect("write string");
    }
    writeln!(out, "- Unreviewed: {}", metrics.audit.unreviewed).expect("write string");

    writeln!(out, "\n### Rules\n").expect("write string");
    writeln!(
        out,
        "| Rule | Reviewed | Structural precision | Actionable precision | False positives |"
    )
    .expect("write string");
    writeln!(out, "| --- | ---: | ---: | ---: | ---: |").expect("write string");
    for (rule, rule_metrics) in &metrics.audit.by_rule {
        writeln!(
            out,
            "| `{rule}` | {} | {} | {} | {} |",
            rule_metrics.reviewed,
            format_optional_percent(rule_metrics.structural_precision),
            format_optional_percent(rule_metrics.actionable_precision),
            rule_metrics.false_positives
        )
        .expect("write string");
    }

    writeln!(out, "\n## Mutations\n").expect("write string");
    writeln!(
        out,
        "- Detected: {}/{}",
        metrics.mutations.detected, metrics.mutations.cases
    )
    .expect("write string");
    writeln!(
        out,
        "- Recall: {}",
        format_optional_percent(metrics.mutations.recall)
    )
    .expect("write string");

    writeln!(out, "\n## Cost\n").expect("write string");
    writeln!(out, "- Index time: {} ms", metrics.total_index_ms).expect("write string");
    writeln!(out, "- Query time: {} ms", metrics.total_query_ms).expect("write string");
    writeln!(out, "- Output bytes: {}", metrics.total_output_bytes).expect("write string");
    writeln!(
        out,
        "- Estimated tokens: {}",
        metrics.total_estimated_tokens
    )
    .expect("write string");
    out
}

fn render_retrieval_row(out: &mut String, label: &str, metrics: &model::RetrievalMetrics) {
    writeln!(
        out,
        "| {label} | {} | {} | {:.1}% | {:.1}% | {:.3} |",
        metrics.cases,
        metrics.correct_cases,
        metrics.mean_recall_at_5 * 100.0,
        metrics.mean_recall_at_10 * 100.0,
        metrics.mean_reciprocal_rank
    )
    .expect("write string");
}

fn render_deltas(deltas: &[BaselineDelta]) -> String {
    if deltas.is_empty() {
        return "# Accuracy Regressions\n\nNo baseline was supplied; this run is observational.\n"
            .to_string();
    }
    let mut out = String::from(
        "# Accuracy Regressions\n\n| Metric | Baseline | Current | Delta |\n| --- | ---: | ---: | ---: |\n",
    );
    for delta in deltas {
        writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            delta.metric,
            format_optional_percent(delta.baseline),
            format_optional_percent(delta.current),
            delta
                .absolute_delta
                .map(|value| format!("{:+.1} pp", value * 100.0))
                .unwrap_or_else(|| "n/a".to_string())
        )
        .expect("write string");
    }
    out
}

fn format_optional_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.1}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_reject_baseline_for_prepare() {
        let args = vec!["--baseline".to_string(), "metrics.json".to_string()];
        assert!(Flags::parse(&args, false).is_err());
    }

    #[test]
    fn baseline_delta_is_new_minus_old() {
        let mut baseline = empty_metrics();
        let mut current = empty_metrics();
        baseline.historical_retrieval.mean_recall_at_10 = 0.5;
        current.historical_retrieval.mean_recall_at_10 = 0.75;

        let deltas = baseline_deltas(&baseline, &current);

        assert_eq!(deltas[0].absolute_delta, Some(0.25));
    }

    fn empty_metrics() -> BenchmarkMetrics {
        BenchmarkMetrics {
            schema_version: SCHEMA_VERSION,
            dataset_version: "v1".to_string(),
            run_id: "run".to_string(),
            generated_at_unix_ms: 0,
            lexa_version: "version".to_string(),
            lexa_git_commit: "commit".to_string(),
            historical_retrieval: model::RetrievalMetrics::default(),
            curated_tools: model::RetrievalMetrics::default(),
            audit: model::AuditMetrics::default(),
            mutations: model::MutationMetrics::default(),
            total_index_ms: 0,
            total_query_ms: 0,
            total_output_bytes: 0,
            total_estimated_tokens: 0,
        }
    }
}
