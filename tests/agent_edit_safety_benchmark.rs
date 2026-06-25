#![allow(clippy::unwrap_used)]

mod common;

use common::{
    assert_all_correct, bench_result_against, parse_json, print_report, run_lexa, run_lexa_fail,
    run_lexa_text_for_json_args, write_fixture, BenchResult,
};
use std::fs;
use std::path::Path;

const SUITE: &str = "edit_safety";

#[test]
fn agent_edit_safety_benchmark_v2() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    write_fixture(project);
    run_lexa(project, &["index", "."]);

    let mut results = Vec::new();
    let read = read_task(project);
    let hash = read.hash;
    results.push(read.result);
    results.push(read_if_hash_task(project, &hash));
    results.push(read_patch_verify_workflow_task(project));
    results.push(patch_dry_run_task(project));
    results.push(patch_real_task(project));
    results.push(patch_stale_hash_task(project, &hash));
    results.push(patch_replace_text_task(project));
    results.push(create_dry_run_task(project));
    results.push(create_real_task(project));
    results.push(create_existing_rejected_task(project));
    results.push(changes_task(project));

    print_report(SUITE, &results);
    assert_all_correct(&results);
}

struct ReadTask {
    result: BenchResult,
    hash: String,
}

fn read_task(project: &Path) -> ReadTask {
    let args = [
        "--json",
        "read",
        "src/agent.rs",
        "--line-start",
        "7",
        "--line-end",
        "10",
    ];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let content = json["content"].as_str().unwrap();
    let correct = json["path"] == "src/agent.rs"
        && json["unchanged"] == false
        && content.contains("pub struct Agent")
        && content.contains("pub engine: Engine");
    let baseline = fs::read_to_string(project.join("src/agent.rs")).unwrap();
    ReadTask {
        hash: json["hash"].as_str().unwrap().to_string(),
        result: bench_result_against(
            SUITE,
            "line-range read",
            "read",
            "full source file read",
            &measured.stdout,
            Some(&baseline),
            correct,
        ),
    }
}

fn read_if_hash_task(project: &Path, hash: &str) -> BenchResult {
    let lexa = run_lexa(
        project,
        &["--json", "read", "src/agent.rs", "--if-hash", hash],
    );
    let measured = run_lexa(project, &["read", "src/agent.rs", "--if-hash", hash]);
    let json = parse_json(&lexa.stdout);
    let correct = json["unchanged"] == true && json["content"] == "";
    bench_result_against(
        SUITE,
        "unchanged hash read",
        "read",
        "Lexa hash cache behavior",
        &measured.stdout,
        None,
        correct,
    )
}

fn read_patch_verify_workflow_task(project: &Path) -> BenchResult {
    let read = run_lexa(project, &["read", "src/orchestrator.rs", "--hash"]);
    let hash = read
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("hash:"))
        .unwrap()
        .to_string();
    let marker = "pub fn workflow_marker() -> usize { 13 }";
    let dry_run = run_lexa(
        project,
        &[
            "patch",
            "src/orchestrator.rs",
            "insert",
            "--after",
            "5",
            "--content",
            marker,
            "--if-hash",
            &hash,
            "--dry-run",
        ],
    );
    let changed = run_lexa(
        project,
        &[
            "patch",
            "src/orchestrator.rs",
            "insert",
            "--after",
            "5",
            "--content",
            marker,
            "--if-hash",
            &hash,
        ],
    );
    let verify = run_lexa(
        project,
        &["read", "src/orchestrator.rs", "--if-hash", &hash],
    );
    let file_content = fs::read_to_string(project.join("src/orchestrator.rs")).unwrap();
    let output = format!(
        "{}{}{}{}",
        read.stdout, dry_run.stdout, changed.stdout, verify.stdout
    );
    let correct = dry_run.stdout.contains(marker)
        && changed
            .stdout
            .contains("edit applied to src/orchestrator.rs")
        && verify.stdout.contains(marker)
        && file_content.contains(marker);
    bench_result_against(
        SUITE,
        "read patch verify workflow",
        "read+patch",
        "Lexa hash-aware edit workflow",
        &output,
        None,
        correct,
    )
}

fn patch_dry_run_task(project: &Path) -> BenchResult {
    let before = fs::read_to_string(project.join("src/config.rs")).unwrap();
    let measured = run_lexa(
        project,
        &[
            "patch",
            "src/config.rs",
            "insert",
            "--after",
            "2",
            "--content",
            "    pub id: usize,",
            "--dry-run",
        ],
    );
    let after = fs::read_to_string(project.join("src/config.rs")).unwrap();
    let correct = measured.stdout.contains("pub id: usize") && before == after;
    bench_result_against(
        SUITE,
        "patch dry-run",
        "patch",
        "Lexa dry-run edit guard",
        &measured.stdout,
        None,
        correct,
    )
}

fn patch_real_task(project: &Path) -> BenchResult {
    let measured = run_lexa(
        project,
        &[
            "patch",
            "src/config.rs",
            "insert",
            "--after",
            "2",
            "--content",
            "    pub id: usize,",
        ],
    );
    let content = fs::read_to_string(project.join("src/config.rs")).unwrap();
    let correct = measured.stdout.contains("edit applied to src/config.rs")
        && content.contains("pub id: usize");
    bench_result_against(
        SUITE,
        "patch real edit",
        "patch",
        "Lexa applied edit guard",
        &measured.stdout,
        None,
        correct,
    )
}

fn patch_stale_hash_task(project: &Path, stale_hash: &str) -> BenchResult {
    let before = fs::read_to_string(project.join("src/config.rs")).unwrap();
    let lexa = run_lexa_fail(
        project,
        &[
            "patch",
            "src/config.rs",
            "insert",
            "--after",
            "2",
            "--content",
            "    pub stale: bool,",
            "--if-hash",
            stale_hash,
        ],
    );
    let after = fs::read_to_string(project.join("src/config.rs")).unwrap();
    let output = format!("{}{}", lexa.stdout, lexa.stderr);
    let correct = output.contains("hash mismatch") && before == after;
    bench_result_against(
        SUITE,
        "stale hash rejection",
        "patch",
        "Lexa stale-hash rejection",
        &output,
        None,
        correct,
    )
}

fn patch_replace_text_task(project: &Path) -> BenchResult {
    let measured = run_lexa(
        project,
        &[
            "patch",
            "docs/agent.md",
            "--replace-text",
            "non-code context",
            "--content",
            "non-code context for benchmark scoring",
        ],
    );
    let content = fs::read_to_string(project.join("docs/agent.md")).unwrap();
    let correct = measured.stdout.contains("edit applied to docs/agent.md")
        && content.contains("non-code context for benchmark scoring");
    bench_result_against(
        SUITE,
        "replace-text edit",
        "patch",
        "Lexa replace-text edit guard",
        &measured.stdout,
        None,
        correct,
    )
}

fn create_dry_run_task(project: &Path) -> BenchResult {
    let measured = run_lexa(
        project,
        &[
            "create",
            "src/generated.rs",
            "--content",
            "pub fn generated() {}\n",
            "--dry-run",
        ],
    );
    let correct =
        measured.stdout.contains("create dry-run") && !project.join("src/generated.rs").exists();
    bench_result_against(
        SUITE,
        "create dry-run",
        "create",
        "Lexa dry-run create guard",
        &measured.stdout,
        None,
        correct,
    )
}

fn create_real_task(project: &Path) -> BenchResult {
    let measured = run_lexa(
        project,
        &[
            "create",
            "src/generated.rs",
            "--content",
            "pub fn generated() {}\n",
        ],
    );
    let content = fs::read_to_string(project.join("src/generated.rs")).unwrap();
    let correct = measured.stdout.contains("file created") && content.contains("generated");
    bench_result_against(
        SUITE,
        "create real file",
        "create",
        "Lexa file creation guard",
        &measured.stdout,
        None,
        correct,
    )
}

fn create_existing_rejected_task(project: &Path) -> BenchResult {
    let before = fs::read_to_string(project.join("src/generated.rs")).unwrap();
    let lexa = run_lexa_fail(
        project,
        &[
            "create",
            "src/generated.rs",
            "--content",
            "pub fn overwritten() {}\n",
        ],
    );
    let after = fs::read_to_string(project.join("src/generated.rs")).unwrap();
    let output = format!("{}{}", lexa.stdout, lexa.stderr);
    let correct = output.contains("file already exists") && before == after;
    bench_result_against(
        SUITE,
        "create existing rejected",
        "create",
        "Lexa overwrite rejection",
        &output,
        None,
        correct,
    )
}

fn changes_task(project: &Path) -> BenchResult {
    let lexa = run_lexa(project, &["changes", "0"]);
    let correct = lexa.stdout.contains("No changes since sequence 0")
        && lexa.stdout.contains("session-local");
    bench_result_against(
        SUITE,
        "session-local changes view",
        "changes",
        "Lexa session change log",
        &lexa.stdout,
        None,
        correct,
    )
}
