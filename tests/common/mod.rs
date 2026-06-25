#![allow(dead_code)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn lexa() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lexa"))
}

#[derive(Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct BenchResult {
    pub suite: &'static str,
    pub task: &'static str,
    pub tool: &'static str,
    pub compared_against: &'static str,
    pub lexa_bytes: usize,
    pub baseline_bytes: Option<usize>,
    pub lexa_tokens: usize,
    pub baseline_tokens: Option<usize>,
    pub correct: bool,
}

pub fn bench_result(
    suite: &'static str,
    task: &'static str,
    tool: &'static str,
    lexa_output: &str,
    baseline_output: Option<&str>,
    correct: bool,
) -> BenchResult {
    bench_result_against(
        suite,
        task,
        tool,
        "n/a",
        lexa_output,
        baseline_output,
        correct,
    )
}

pub fn bench_result_against(
    suite: &'static str,
    task: &'static str,
    tool: &'static str,
    compared_against: &'static str,
    lexa_output: &str,
    baseline_output: Option<&str>,
    correct: bool,
) -> BenchResult {
    BenchResult {
        suite,
        task,
        tool,
        compared_against,
        lexa_bytes: lexa_output.len(),
        baseline_bytes: baseline_output.map(str::len),
        lexa_tokens: estimated_tokens(lexa_output),
        baseline_tokens: baseline_output.map(estimated_tokens),
        correct,
    }
}

pub fn run_lexa(project: &Path, args: &[&str]) -> CommandResult {
    let output = lexa().current_dir(project).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "lexa {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    CommandResult {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

pub fn run_lexa_fail(project: &Path, args: &[&str]) -> CommandResult {
    let output = lexa().current_dir(project).args(args).output().unwrap();
    assert!(
        !output.status.success(),
        "lexa {:?} unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    CommandResult {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

pub fn run_lexa_text_for_json_args(project: &Path, args: &[&str]) -> CommandResult {
    let text_args = args
        .iter()
        .copied()
        .filter(|arg| *arg != "--json")
        .collect::<Vec<_>>();
    run_lexa(project, &text_args)
}

pub fn parse_json(output: &str) -> Value {
    serde_json::from_str(output).unwrap()
}

pub fn estimated_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

pub fn line_overlap(symbol: &Value, expected_start: u64, expected_end: u64) -> bool {
    let line_start = symbol["line_start"].as_u64().unwrap();
    let line_end = symbol["line_end"].as_u64().unwrap();
    line_start <= expected_end && expected_start <= line_end
}

pub fn grep_like(project: &Path, terms: &[&str], path: Option<&str>) -> String {
    let mut output = String::new();
    let files = if let Some(path) = path {
        vec![project.join(path)]
    } else {
        collect_fixture_files(project)
    };

    for file in files {
        let relative = file.strip_prefix(project).unwrap().to_string_lossy();
        let content = fs::read_to_string(&file).unwrap();
        for (index, line) in content.lines().enumerate() {
            if terms.iter().any(|term| line.contains(term)) {
                output.push_str(&format!("{}:{}:{}\n", relative, index + 1, line));
            }
        }
    }

    output
}

pub fn grep_like_with_candidate_reads(
    project: &Path,
    terms: &[&str],
    path: Option<&str>,
) -> String {
    let mut output = grep_like(project, terms, path);
    let mut matched_files = Vec::new();
    let files = if let Some(path) = path {
        vec![project.join(path)]
    } else {
        collect_fixture_files(project)
    };

    for file in files {
        let content = fs::read_to_string(&file).unwrap();
        if content
            .lines()
            .any(|line| terms.iter().any(|term| line.contains(term)))
        {
            matched_files.push(file);
        }
    }

    output.push_str("\n# candidate file reads\n");
    for file in matched_files {
        let relative = file.strip_prefix(project).unwrap().to_string_lossy();
        output.push_str(&format!("=== {} ===\n", relative));
        output.push_str(&fs::read_to_string(file).unwrap());
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }

    output
}

pub fn collect_fixture_files(project: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files(project, project, &mut files);
    files.sort();
    files
}

fn collect_files(project: &Path, dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = path.strip_prefix(project).unwrap();
        if relative.starts_with(".lexa") {
            continue;
        }
        if path.is_dir() {
            collect_files(project, &path, files);
        } else {
            files.push(path);
        }
    }
}

pub fn print_report(suite: &str, results: &[BenchResult]) {
    let passed = results.iter().filter(|result| result.correct).count();
    let lexa_tokens: usize = results.iter().map(|result| result.lexa_tokens).sum();
    let baseline_tokens: usize = results
        .iter()
        .filter_map(|result| result.baseline_tokens)
        .sum();
    let reduction = if baseline_tokens == 0 {
        None
    } else {
        Some(1.0 - (lexa_tokens as f64 / baseline_tokens as f64))
    };

    println!("\nAgent benchmark v2: {suite}");
    println!(
        "summary: {passed}/{} correct, lexa_tokens={lexa_tokens}, baseline_tokens={}",
        results.len(),
        if baseline_tokens == 0 {
            "n/a".to_string()
        } else {
            baseline_tokens.to_string()
        }
    );
    if let Some(reduction) = reduction {
        println!("aggregate reduction: {:.1}%", reduction * 100.0);
    }
    println!(
        "| suite | task | tool | compared against | Lexa bytes | baseline bytes | Lexa est. tokens | baseline est. tokens | reduction | correct |"
    );
    println!("| --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- |");
    for result in results {
        let baseline_bytes = result
            .baseline_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_string());
        let baseline_tokens = result
            .baseline_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_string());
        let reduction = match result.baseline_tokens {
            Some(tokens) if tokens > 0 => {
                format!(
                    "{:.1}%",
                    (1.0 - result.lexa_tokens as f64 / tokens as f64) * 100.0
                )
            }
            _ => "n/a".to_string(),
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            result.suite,
            result.task,
            result.tool,
            result.compared_against,
            result.lexa_bytes,
            baseline_bytes,
            result.lexa_tokens,
            baseline_tokens,
            reduction,
            result.correct
        );
    }
}

pub fn assert_all_correct(results: &[BenchResult]) {
    for result in results {
        assert!(
            result.correct,
            "{} / {} failed correctness check",
            result.suite, result.task
        );
    }
}

pub fn write_fixture(project: &Path) {
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(project.join("tests")).unwrap();
    fs::create_dir_all(project.join("docs")).unwrap();

    write(
        project,
        "lexa.toml",
        r#"[audit.thresholds]
large_file_warning = 12
large_file_high = 40
large_symbol_warning = 8
large_symbol_high = 30
fan_in_warning = 2
fan_in_high = 4
fan_out_warning = 3
fan_out_high = 8

[audit.rules]
"dead_code.candidate" = "warning"
"#,
    );
    write(
        project,
        "src/runtime.rs",
        r#"pub enum Runtime {
    Local,
    Remote,
}
"#,
    );
    write(
        project,
        "src/config.rs",
        r#"pub struct AgentConfig {
    pub name: String,
}

impl AgentConfig {
    pub fn demo() -> Self {
        Self {
            name: "demo".to_string(),
        }
    }
}
"#,
    );
    write(
        project,
        "src/engine.rs",
        r#"use crate::runtime::Runtime;

pub struct Engine {
    runtime: Runtime,
}

impl Engine {
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}
"#,
    );
    write(
        project,
        "src/agent.rs",
        r#"use crate::config::AgentConfig;
use crate::engine::Engine;
use crate::runtime::Runtime;

pub type ProjectAgent = Agent;

pub struct Agent {
    pub name: String,
    pub engine: Engine,
}

pub fn create_project_agent(config: AgentConfig) -> Agent {
    Agent {
        name: config.name,
        engine: Engine::new(Runtime::Local),
    }
}

pub fn create_demo_agent() -> Agent {
    create_project_agent(AgentConfig::demo())
}
"#,
    );
    write(
        project,
        "src/app.rs",
        r#"use crate::agent::{create_demo_agent, create_project_agent};
use crate::config::AgentConfig;

pub fn boot() {
    let _agent = create_project_agent(AgentConfig::demo());
}

pub fn demo() {
    let _agent = create_demo_agent();
}
"#,
    );
    write(
        project,
        "src/orchestrator.rs",
        r#"use crate::agent::create_project_agent;
use crate::config::AgentConfig;

pub fn orchestrate(config: AgentConfig) {
    let _agent = create_project_agent(config);
}
"#,
    );
    write(
        project,
        "src/context.rs",
        r#"use crate::agent::Agent;

pub fn build_context_bundle(agent: &Agent) -> String {
    format!("agent:{}", agent.name)
}

pub fn build_context_summary(agent: &Agent) -> String {
    build_context_bundle(agent)
}
"#,
    );
    write(
        project,
        "src/lib.rs",
        r#"pub mod agent;
pub mod app;
pub mod config;
pub mod context;
pub mod engine;
pub mod orchestrator;
pub mod runtime;
"#,
    );
    write(
        project,
        "src/app.ts",
        r#"import { createProjectAgent } from "./web_agent";
import { AgentConfig } from "./web_config";
import { Runtime } from "./web_runtime";

export function boot(config: AgentConfig) {
  return createProjectAgent(config, Runtime.Local);
}
"#,
    );
    write(
        project,
        "src/web_agent.ts",
        r#"import { AgentConfig } from "./web_config";
import { Runtime } from "./web_runtime";

export function createProjectAgent(config: AgentConfig, runtime: Runtime) {
  return { config, runtime };
}
"#,
    );
    write(
        project,
        "src/web_config.ts",
        r#"export interface AgentConfig {
  name: string;
}
"#,
    );
    write(
        project,
        "src/web_runtime.ts",
        r#"export enum Runtime {
  Local = "local",
  Remote = "remote",
}
"#,
    );
    write(
        project,
        "src/broken.ts",
        r#"import { MissingThing } from "./missing";

export function useMissing(value: MissingThing) {
  return value;
}
"#,
    );
    write(
        project,
        "src/unused.rs",
        r#"pub fn unused_internal_helper() -> usize {
    42
}
"#,
    );
    write(
        project,
        "tests/agent_notes.rs",
        r#"// Engine appears here as documentation noise for grep-style search.
// create_project_agent appears here as non-call noise.
"#,
    );
    write(
        project,
        "docs/agent.md",
        r#"# Agent notes

Engine, build, context, create, project, and agent are repeated here to make
plain text scanning return non-code context that an agent still has to filter.
"#,
    );

    for index in 0..8 {
        write(
            project,
            &format!("docs/noise_{index}.md"),
            &format!(
                "# Noise {index}\n\nEngine build context create project agent notes.\n\
This document repeats Agent, Engine, ProjectAgent, create, project, and agent words.\n\
It is intentionally not executable code and should not be selected as source truth.\n"
            ),
        );
        write(
            project,
            &format!("tests/noise_{index}.rs"),
            &format!(
                "// Engine and create_project_agent are mentioned in test notes {index}.\n\
// build context create project agent terms appear here as non-call noise.\n\
fn unrelated_noise_{index}() {{}}\n"
            ),
        );
    }
}

pub fn write(project: &Path, relative: &str, content: &str) {
    if let Some(parent) = project.join(relative).parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(project.join(relative), content).unwrap();
}
