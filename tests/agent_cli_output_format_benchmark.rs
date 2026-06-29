#![allow(clippy::unwrap_used)]

mod common;

use common::{estimated_tokens, parse_json, run_lexa, run_lexa_text_for_json_args, write_fixture};
use serde_json::Value;
use std::path::Path;

const SUITE: &str = "cli_output_format";

#[derive(Debug)]
struct CliCase {
    task: &'static str,
    tool: &'static str,
    json_args: &'static [&'static str],
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
    text: FormatMeasurement,
    json: FormatMeasurement,
    toon: FormatMeasurement,
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
            json_args: &[
                "--json",
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
            json_args: &["--json", "list", "src"],
        },
        CliCase {
            task: "glob paths",
            tool: "glob",
            json_args: &["--json", "glob", "src/*.ts"],
        },
        CliCase {
            task: "fuzzy path",
            tool: "path_search",
            json_args: &["--json", "path-search", "web_agent", "--max-results", "5"],
        },
        CliCase {
            task: "scoped text search",
            tool: "text_search",
            json_args: &[
                "--json",
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
            json_args: &["--json", "word-refs", "ProjectAgent"],
        },
        CliCase {
            task: "exact definition",
            tool: "symbol_defs",
            json_args: &["--json", "symbol-defs", "Engine"],
        },
        CliCase {
            task: "fuzzy symbol",
            tool: "symbol_search",
            json_args: &[
                "--json",
                "symbol-search",
                "build context",
                "--max-results",
                "5",
            ],
        },
        CliCase {
            task: "callers",
            tool: "callers",
            json_args: &[
                "--json",
                "callers",
                "create_project_agent",
                "--max-results",
                "20",
            ],
        },
        CliCase {
            task: "outline",
            tool: "outline",
            json_args: &["--json", "outline", "src/agent.rs"],
        },
        CliCase {
            task: "dependencies",
            tool: "trace_deps",
            json_args: &["--json", "trace-deps", "src/app.ts"],
        },
        CliCase {
            task: "brief",
            tool: "brief",
            json_args: &[
                "--json",
                "brief",
                "create project agent",
                "--max-results",
                "5",
            ],
        },
        CliCase {
            task: "composed query",
            tool: "pipeline",
            json_args: &[
                "--json",
                "pipeline",
                "glob src/*.rs | search create_project_agent | limit 5",
            ],
        },
    ]
}

fn measure_case(project: &Path, case: &CliCase) -> FormatBenchResult {
    let text = run_lexa_text_for_json_args(project, case.json_args).stdout;
    let json = run_lexa(project, case.json_args).stdout;
    let json_value = parse_json(&json);
    let toon = encode_toon(&json_value);

    FormatBenchResult {
        task: case.task,
        tool: case.tool,
        text: measure(&text),
        json: measure(&json),
        toon: measure(&toon),
    }
}

fn encode_toon(value: &Value) -> String {
    toon_format::encode_default(value).unwrap()
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
        "summary: text_tokens={}, json_tokens={}, toon_tokens={}",
        totals.text.tokens, totals.json.tokens, totals.toon.tokens
    );
    println!(
        "aggregate: toon_vs_json={}, toon_vs_text={}, json_vs_text={}",
        reduction(totals.toon.tokens, totals.json.tokens),
        reduction(totals.toon.tokens, totals.text.tokens),
        reduction(totals.json.tokens, totals.text.tokens)
    );
    println!(
        "| suite | task | tool | text bytes | json bytes | toon bytes | text est. tokens | json est. tokens | toon est. tokens | toon vs json | toon vs text | json vs text |"
    );
    println!("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for result in results {
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            suite,
            result.task,
            result.tool,
            result.text.bytes,
            result.json.bytes,
            result.toon.bytes,
            result.text.tokens,
            result.json.tokens,
            result.toon.tokens,
            reduction(result.toon.tokens, result.json.tokens),
            reduction(result.toon.tokens, result.text.tokens),
            reduction(result.json.tokens, result.text.tokens)
        );
    }
    println!(
        "| {} | TOTAL | all | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        suite,
        totals.text.bytes,
        totals.json.bytes,
        totals.toon.bytes,
        totals.text.tokens,
        totals.json.tokens,
        totals.toon.tokens,
        reduction(totals.toon.tokens, totals.json.tokens),
        reduction(totals.toon.tokens, totals.text.tokens),
        reduction(totals.json.tokens, totals.text.tokens)
    );
}

fn totals(results: &[FormatBenchResult]) -> FormatBenchResult {
    FormatBenchResult {
        task: "TOTAL",
        tool: "all",
        text: sum_measurements(results.iter().map(|result| &result.text)),
        json: sum_measurements(results.iter().map(|result| &result.json)),
        toon: sum_measurements(results.iter().map(|result| &result.toon)),
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

fn reduction(candidate_tokens: usize, baseline_tokens: usize) -> String {
    if baseline_tokens == 0 {
        return "n/a".to_string();
    }
    format!(
        "{:.1}%",
        (1.0 - candidate_tokens as f64 / baseline_tokens as f64) * 100.0
    )
}
