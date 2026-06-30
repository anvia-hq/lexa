#![allow(clippy::unwrap_used)]

mod common;

use common::{
    assert_all_correct, bench_result_against, grep_like, grep_like_with_candidate_reads,
    line_overlap, parse_json, print_report, run_lexa, run_lexa_text_for_json_args, write_fixture,
    BenchResult,
};
use std::fs;
use std::path::Path;

const SUITE: &str = "retrieval";

#[test]
fn agent_retrieval_benchmark_v2() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    write_fixture(project);
    run_lexa(project, &["index", "."]);

    let results = vec![
        files_task(project),
        list_task(project),
        glob_task(project),
        path_search_task(project),
        text_search_task(project),
        word_refs_task(project),
        exact_definition_task(project),
        fuzzy_symbol_task(project),
        callers_task(project),
        outline_task(project),
        dependency_task(project),
        brief_task(project),
        pipeline_task(project),
    ];

    print_report(SUITE, &results);
    assert_all_correct(&results);
}

fn files_task(project: &Path) -> BenchResult {
    let args = [
        "files",
        "src",
        "--language",
        "rust",
        "--min-lines",
        "4",
        "--max-results",
        "20",
    ];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let files = json["files"].as_array().unwrap();
    let correct = files.iter().any(|file| file["path"] == "src/agent.rs")
        && files.iter().all(|file| file["language"] == "rust")
        && json["count"].as_u64().unwrap() <= 20;
    let baseline = list_files(project, Some("src"), Some(".rs"));
    bench_result_against(
        SUITE,
        "filtered file overview",
        "files",
        "recursive file listing filtered to src/**/*.rs",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn list_task(project: &Path) -> BenchResult {
    let args = ["list", "src"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let entries = json["entries"].as_array().unwrap();
    let correct = entries
        .iter()
        .any(|entry| entry["name"] == "agent.rs" && entry["kind"] == "file")
        && !entries.iter().any(|entry| entry["name"] == "noise_0.md");
    let baseline = list_files(project, Some("src"), None);
    bench_result_against(
        SUITE,
        "directory children",
        "list",
        "recursive file listing under src",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn glob_task(project: &Path) -> BenchResult {
    let args = ["glob", "src/*.ts"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let paths = json["paths"].as_array().unwrap();
    let correct = paths.iter().any(|path| path.as_str() == Some("src/app.ts"))
        && paths
            .iter()
            .all(|path| path.as_str().unwrap().starts_with("src/"));
    let baseline = list_files(project, Some("src"), Some(".ts"));
    bench_result_against(
        SUITE,
        "glob paths",
        "glob",
        "file listing filtered to src/*.ts",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn path_search_task(project: &Path) -> BenchResult {
    let args = ["path-search", "web_agent", "--max-results", "5"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|result| result["path"] == "src/web_agent.ts");
    let baseline = list_files(project, None, None);
    bench_result_against(
        SUITE,
        "fuzzy path",
        "path_search",
        "full file listing for agent-side fuzzy matching",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn text_search_task(project: &Path) -> BenchResult {
    let args = [
        "text-search",
        "create_project_agent",
        "--scope",
        "--compact",
        "--path-glob",
        "src/*.rs",
    ];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["results"].as_array().unwrap().iter().any(|result| {
        result["path"] == "src/app.rs"
            && result["scope"]["name"]
                .as_str()
                .is_some_and(|name| name == "boot")
    });
    let baseline =
        grep_like_with_candidate_reads(project, &["create_project_agent"], Some("src/app.rs"));
    bench_result_against(
        SUITE,
        "scoped text search",
        "text_search",
        "scoped grep plus candidate file read",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn word_refs_task(project: &Path) -> BenchResult {
    let args = ["word-refs", "ProjectAgent"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|result| result["path"] == "src/agent.rs" && result["line_num"] == 5);
    let baseline = grep_like(project, &["ProjectAgent"], None);
    bench_result_against(
        SUITE,
        "exact word refs",
        "word_refs",
        "grep exact word across project",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn exact_definition_task(project: &Path) -> BenchResult {
    let args = ["symbol-defs", "Engine"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["results"].as_array().unwrap().iter().any(|result| {
        result["path"] == "src/engine.rs"
            && result["symbol"]["name"] == "Engine"
            && result["symbol"]["kind"] == "struct"
            && line_overlap(&result["symbol"], 3, 5)
    });
    let baseline = grep_like_with_candidate_reads(project, &["Engine"], None);
    bench_result_against(
        SUITE,
        "exact definition",
        "symbol_defs",
        "grep symbol name plus candidate file reads",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn fuzzy_symbol_task(project: &Path) -> BenchResult {
    let args = ["symbol-search", "build context", "--max-results", "5"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["results"].as_array().unwrap().iter().any(|result| {
        result["path"] == "src/context.rs" && result["name"] == "build_context_bundle"
    });
    let baseline = grep_like_with_candidate_reads(project, &["build", "context"], None);
    bench_result_against(
        SUITE,
        "fuzzy symbol",
        "symbol_search",
        "grep query terms plus candidate file reads",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn callers_task(project: &Path) -> BenchResult {
    let args = ["callers", "create_project_agent", "--max-results", "20"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let results = json["results"].as_array().unwrap();
    let has_app = results.iter().any(|result| {
        result["path"] == "src/app.rs"
            && result["line_text"]
                .as_str()
                .unwrap()
                .contains("create_project_agent(AgentConfig::demo())")
    });
    let has_orchestrator = results.iter().any(|result| {
        result["path"] == "src/orchestrator.rs"
            && result["line_text"]
                .as_str()
                .unwrap()
                .contains("create_project_agent(config)")
    });
    let excludes_definition = results.iter().all(|result| {
        !(result["path"] == "src/agent.rs"
            && result["line_text"]
                .as_str()
                .unwrap()
                .contains("pub fn create_project_agent"))
    });
    let correct = has_app && has_orchestrator && excludes_definition;
    let baseline = grep_like_with_candidate_reads(project, &["create_project_agent"], None);
    bench_result_against(
        SUITE,
        "callers",
        "callers",
        "grep symbol name plus candidate file reads",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn outline_task(project: &Path) -> BenchResult {
    let args = ["outline", "src/agent.rs"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let symbols = json["symbols"].as_array().unwrap();
    let imports = json["imports"].as_array().unwrap();
    let correct = symbols
        .iter()
        .any(|symbol| symbol["name"] == "Agent" && symbol["kind"] == "struct")
        && symbols
            .iter()
            .any(|symbol| symbol["name"] == "create_project_agent" && symbol["kind"] == "function")
        && imports
            .iter()
            .any(|import| import.as_str().unwrap().contains("crate::engine::Engine"));
    let baseline = fs::read_to_string(project.join("src/agent.rs")).unwrap();
    bench_result_against(
        SUITE,
        "outline",
        "outline",
        "full source file read",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn dependency_task(project: &Path) -> BenchResult {
    let args = ["trace-deps", "src/app.ts"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let dependencies = json["dependencies"].as_array().unwrap();
    let expected = [
        "src/web_agent.ts",
        "src/web_config.ts",
        "src/web_runtime.ts",
    ];
    let correct = expected.iter().all(|path| {
        dependencies
            .iter()
            .any(|dependency| dependency.as_str() == Some(path))
    });
    let baseline =
        grep_like_with_candidate_reads(project, &["import", "from", "require"], Some("src/app.ts"));
    bench_result_against(
        SUITE,
        "dependencies",
        "trace_deps",
        "grep imports/requires plus candidate file read",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn brief_task(project: &Path) -> BenchResult {
    let args = ["brief", "create project agent", "--max-results", "5"];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["relevant_symbols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|symbol| {
            symbol["path"] == "src/agent.rs"
                && symbol["name"] == "create_project_agent"
                && symbol["kind"] == "function"
        });
    let baseline = grep_like_with_candidate_reads(project, &["create", "project", "agent"], None);
    bench_result_against(
        SUITE,
        "brief",
        "brief",
        "grep query terms plus candidate file reads",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn pipeline_task(project: &Path) -> BenchResult {
    let args = [
        "pipeline",
        "glob src/*.rs | search create_project_agent | limit 5",
    ];
    let lexa = run_lexa(project, &args);
    let measured = run_lexa_text_for_json_args(project, &args);
    let json = parse_json(&lexa.stdout);
    let correct = json["result_type"] == "results"
        && json["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == "src/app.rs" || item["path"] == "src/agent.rs");
    let baseline = grep_like_with_candidate_reads(project, &["create_project_agent"], None);
    bench_result_against(
        SUITE,
        "composed query",
        "pipeline",
        "grep symbol name plus candidate file reads",
        &measured.stdout,
        Some(&baseline),
        correct,
    )
}

fn list_files(project: &Path, prefix: Option<&str>, suffix: Option<&str>) -> String {
    let mut output = String::new();
    for file in common::collect_fixture_files(project) {
        let relative = file.strip_prefix(project).unwrap().to_string_lossy();
        if prefix.is_some_and(|prefix| !relative.starts_with(prefix)) {
            continue;
        }
        if suffix.is_some_and(|suffix| !relative.ends_with(suffix)) {
            continue;
        }
        output.push_str(&relative);
        output.push('\n');
    }
    output
}
