#![allow(clippy::unwrap_used)]

mod common;

use common::{estimated_tokens, parse_toon, run_lexa, write_fixture};
use std::path::Path;

const SUITE: &str = "cli_output_format";

#[derive(Debug)]
struct CliCase {
    task: &'static str,
    tool: &'static str,
    args: &'static [&'static str],
}

#[derive(Debug)]
struct FormatMeasurement {
    bytes: usize,
    tokens: usize,
}

#[derive(Debug)]
struct FormatBenchResult {
    task: &'static str,
    tool: &'static str,
    current: FormatMeasurement,
}

#[test]
fn agent_cli_output_format_benchmark_v1() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    write_fixture(project);
    run_lexa(project, &["index", "."]);

    let results = cli_cases()
        .iter()
        .map(|case| measure_case(project, case))
        .collect::<Vec<_>>();

    print_format_report(SUITE, &results);
}

fn cli_cases() -> Vec<CliCase> {
    vec![
        CliCase {
            task: "filtered file overview",
            tool: "files",
            args: &[
                "files",
                "src",
                "--language",
                "rust",
                "--min-lines",
                "4",
                "--max-results",
                "20",
            ],
        },
        CliCase {
            task: "directory children",
            tool: "list",
            args: &["list", "src"],
        },
        CliCase {
            task: "glob paths",
            tool: "glob",
            args: &["glob", "src/*.ts"],
        },
        CliCase {
            task: "fuzzy path",
            tool: "path_search",
            args: &["path-search", "web_agent", "--max-results", "5"],
        },
        CliCase {
            task: "scoped text search",
            tool: "text_search",
            args: &[
                "text-search",
                "create_project_agent",
                "--scope",
                "--compact",
                "--path-glob",
                "src/*.rs",
            ],
        },
        CliCase {
            task: "exact word refs",
            tool: "word_refs",
            args: &["word-refs", "ProjectAgent"],
        },
        CliCase {
            task: "exact definition",
            tool: "symbol_defs",
            args: &["symbol-defs", "Engine"],
        },
        CliCase {
            task: "fuzzy symbol",
            tool: "symbol_search",
            args: &["symbol-search", "build context", "--max-results", "5"],
        },
        CliCase {
            task: "callers",
            tool: "callers",
            args: &["callers", "create_project_agent", "--max-results", "20"],
        },
        CliCase {
            task: "outline",
            tool: "outline",
            args: &["outline", "src/agent.rs"],
        },
        CliCase {
            task: "dependencies",
            tool: "trace_deps",
            args: &["trace-deps", "src/app.ts"],
        },
        CliCase {
            task: "brief",
            tool: "brief",
            args: &["brief", "create project agent", "--max-results", "5"],
        },
        CliCase {
            task: "composed query",
            tool: "pipeline",
            args: &[
                "pipeline",
                "glob src/*.rs | search create_project_agent | limit 5",
            ],
        },
    ]
}

fn measure_case(project: &Path, case: &CliCase) -> FormatBenchResult {
    let output = run_lexa(project, case.args).stdout;
    let decoded = parse_toon(&output);
    assert_eq!(decoded["tool"], case.tool);
    assert!(decoded.get("ok").is_none() || decoded["ok"] == true);

    FormatBenchResult {
        task: case.task,
        tool: case.tool,
        current: measure(&output),
    }
}

fn measure(output: &str) -> FormatMeasurement {
    FormatMeasurement {
        bytes: output.len(),
        tokens: estimated_tokens(output),
    }
}

fn print_format_report(suite: &str, results: &[FormatBenchResult]) {
    let totals = totals(results);

    println!("\nAgent output format benchmark: {suite}");
    println!(
        "summary: current_tokens={}, current_bytes={}",
        totals.current.tokens, totals.current.bytes
    );
    println!("| suite | task | tool | current bytes | current est. tokens |");
    println!("| --- | --- | --- | ---: | ---: |");
    for result in results {
        println!(
            "| {} | {} | {} | {} | {} |",
            suite, result.task, result.tool, result.current.bytes, result.current.tokens,
        );
    }
    println!(
        "| {} | TOTAL | all | {} | {} |",
        suite, totals.current.bytes, totals.current.tokens,
    );
}

fn totals(results: &[FormatBenchResult]) -> FormatBenchResult {
    FormatBenchResult {
        task: "TOTAL",
        tool: "all",
        current: sum_measurements(results.iter().map(|result| &result.current)),
    }
}

fn sum_measurements<'a>(items: impl Iterator<Item = &'a FormatMeasurement>) -> FormatMeasurement {
    items.fold(
        FormatMeasurement {
            bytes: 0,
            tokens: 0,
        },
        |acc, item| FormatMeasurement {
            bytes: acc.bytes + item.bytes,
            tokens: acc.tokens + item.tokens,
        },
    )
}
