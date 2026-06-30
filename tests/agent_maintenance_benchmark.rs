#![allow(clippy::unwrap_used)]

mod common;

use common::{
    assert_all_correct, bench_result_against, parse_json, print_report, run_lexa,
    run_lexa_text_for_json_args, write, write_fixture, BenchResult,
};
use std::path::Path;

const SUITE: &str = "maintenance";

#[test]
fn agent_maintenance_benchmark_v2() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    write_fixture(project);
    run_lexa(project, &["index", "."]);

    let results = vec![
        recent_task(project),
        status_task(project),
        reindex_task(project),
        audit_task(project),
        clear_index_task(project),
    ];

    print_report(SUITE, &results);
    assert_all_correct(&results);
}

fn recent_task(project: &Path) -> BenchResult {
    write(
        project,
        "src/recent.rs",
        "pub fn recently_added() -> usize { 7 }\n",
    );
    let args = ["recent", "--limit", "5"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "src/recent.rs");
    bench_result_against(
        SUITE,
        "recent file refresh",
        "recent",
        "Lexa index state refresh",
        &measured.stdout,
        None,
        correct,
    )
}

fn status_task(project: &Path) -> BenchResult {
    let args = ["status"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["files_indexed"].as_u64().unwrap() >= 1
        && json["symbols_indexed"].as_u64().unwrap() >= 1
        && json["graph"]["exists"] == true;
    bench_result_against(
        SUITE,
        "index status",
        "status",
        "Lexa index status",
        &measured.stdout,
        None,
        correct,
    )
}

fn reindex_task(project: &Path) -> BenchResult {
    write(
        project,
        "src/reindexed.rs",
        "pub fn reindexed_symbol() -> usize { 9 }\n",
    );
    let measured = run_lexa(project, &["reindex", "."]);
    let symbol = run_lexa(project, &["symbol-defs", "reindexed_symbol"]);
    let reindex_json = parse_json(&measured.stdout);
    let symbol_json = parse_json(&symbol.stdout);
    let correct = reindex_json["files_indexed"].as_u64().unwrap() >= 1
        && symbol_json["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["path"] == "src/reindexed.rs");
    bench_result_against(
        SUITE,
        "rebuild index",
        "reindex",
        "Lexa index rebuild",
        &measured.stdout,
        None,
        correct,
    )
}

fn audit_task(project: &Path) -> BenchResult {
    let args = [
        "audit",
        "--config",
        "lexa.toml",
        "--include",
        "dead-code",
        "--max",
        "50",
    ];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let findings = json["findings"].as_array().unwrap();
    let has_unresolved = findings.iter().any(|finding| {
        finding["rule"] == "dependency.unresolved_import" && finding["path"] == "src/broken.ts"
    });
    let has_dead_code = findings.iter().any(|finding| {
        finding["rule"] == "dead_code.candidate"
            && finding["path"].as_str().unwrap().starts_with("src/")
    });
    let correct = json["summary"]["total_findings"].as_u64().unwrap() > 0
        && (has_unresolved || has_dead_code);
    bench_result_against(
        SUITE,
        "architecture audit",
        "audit",
        "Lexa architecture audit",
        &measured.stdout,
        None,
        correct,
    )
}

fn clear_index_task(project: &Path) -> BenchResult {
    let measured = run_lexa(project, &["clear-index"]);
    let json = parse_json(&measured.stdout);
    let correct = json["removed"] == true && !project.join(".lexa/graph.lexa").exists();
    bench_result_against(
        SUITE,
        "clear persisted graph",
        "clear_index",
        "Lexa index clear",
        &measured.stdout,
        None,
        correct,
    )
}
