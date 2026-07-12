use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const START: &str = "<!-- TOOLS START -->\n";
const END: &str = "<!-- TOOLS END -->\n";

#[derive(Debug, Deserialize)]
struct ToolSpec {
    name: String,
    summary: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Deserialize)]
struct CriterionEstimates {
    mean: CriterionMean,
}

#[derive(Debug, Deserialize)]
struct CriterionMean {
    point_estimate: f64,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        bail!("usage: xtask <gen-skill [--check] | perf-gate>");
    };

    match command.as_str() {
        "gen-skill" => match args.collect::<Vec<_>>().as_slice() {
            [] => gen_skill(false),
            [flag] if flag == "--check" => gen_skill(true),
            _ => bail!("usage: xtask gen-skill [--check]"),
        },
        "perf-gate" => match args.collect::<Vec<_>>().as_slice() {
            [] => perf_gate(),
            _ => bail!("usage: xtask perf-gate"),
        },
        other => bail!("unknown xtask command: {other}"),
    }
}

fn perf_gate() -> Result<()> {
    const BASELINE_REF: &str = "v0.9.0";
    const REQUIRED_IMPROVEMENT: f64 = 0.20;
    const BENCH_FILTER: &str = "project_index/500|search/exact_word";
    const METRICS: [(&str, &str); 2] = [
        ("500-file indexing", "project_index/500"),
        ("warm exact search", "search/exact_word"),
    ];

    let repo_root = find_repo_root()?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_root =
        std::env::temp_dir().join(format!("lexa-perf-gate-{}-{nonce}", std::process::id()));
    let baseline_root = temp_root.join("baseline");
    let target_dir = temp_root.join("target");
    std::fs::create_dir_all(&temp_root)
        .with_context(|| format!("failed to create {}", temp_root.display()))?;

    let result = (|| -> Result<()> {
        run_command(
            Command::new("git")
                .args(["worktree", "add", "--detach"])
                .arg(&baseline_root)
                .arg(BASELINE_REF)
                .current_dir(&repo_root),
            "create v0.9 performance worktree",
        )?;

        run_bench(&baseline_root, &target_dir, BENCH_FILTER)?;
        let baseline = METRICS
            .iter()
            .map(|(_, path)| read_criterion_mean(&target_dir, path))
            .collect::<Result<Vec<_>>>()?;

        run_bench(&repo_root, &target_dir, BENCH_FILTER)?;
        let current = METRICS
            .iter()
            .map(|(_, path)| read_criterion_mean(&target_dir, path))
            .collect::<Result<Vec<_>>>()?;

        let mut failed = false;
        for (((label, _), baseline_ns), current_ns) in METRICS.iter().zip(baseline).zip(current) {
            let improvement = 1.0 - current_ns / baseline_ns;
            let passed = improvement >= REQUIRED_IMPROVEMENT;
            println!(
                "{label}: v0.9 {}, current {}, improvement {:.1}% [{}]",
                format_duration_ns(baseline_ns),
                format_duration_ns(current_ns),
                improvement * 100.0,
                if passed { "PASS" } else { "FAIL" }
            );
            failed |= !passed;
        }

        if failed {
            bail!(
                "performance gate failed: every metric must improve by at least {:.0}% over {BASELINE_REF}",
                REQUIRED_IMPROVEMENT * 100.0
            );
        }
        Ok(())
    })();

    let cleanup = Command::new("git")
        .args(["worktree", "remove"])
        .arg(&baseline_root)
        .current_dir(&repo_root)
        .status();
    let cleanup_failed = match cleanup {
        Ok(status) => !status.success(),
        Err(_) => true,
    };
    if baseline_root.exists() && cleanup_failed {
        eprintln!(
            "warning: failed to remove temporary worktree {}",
            baseline_root.display()
        );
    }
    if let Err(err) = std::fs::remove_dir_all(&temp_root) {
        eprintln!(
            "warning: failed to remove temporary benchmark data {}: {err}",
            temp_root.display()
        );
    }

    result
}

fn run_bench(repo_root: &Path, target_dir: &Path, filter: &str) -> Result<()> {
    run_command(
        Command::new("cargo")
            .args(["bench", "--bench", "engine", "--"])
            .arg(filter)
            .arg("--noplot")
            .env("CARGO_TARGET_DIR", target_dir)
            .current_dir(repo_root),
        &format!("run performance benchmarks in {}", repo_root.display()),
    )
}

fn run_command(command: &mut Command, description: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to {description}"))?;
    if !status.success() {
        bail!("{description} failed with {status}");
    }
    Ok(())
}

fn read_criterion_mean(target_dir: &Path, benchmark: &str) -> Result<f64> {
    let path = target_dir
        .join("criterion")
        .join(benchmark)
        .join("new")
        .join("estimates.json");
    let encoded = std::fs::read(&path)
        .with_context(|| format!("failed to read benchmark result {}", path.display()))?;
    let estimates: CriterionEstimates = serde_json::from_slice(&encoded)
        .with_context(|| format!("failed to parse benchmark result {}", path.display()))?;
    Ok(estimates.mean.point_estimate)
}

fn format_duration_ns(nanoseconds: f64) -> String {
    if nanoseconds >= 1_000_000.0 {
        format!("{:.2} ms", nanoseconds / 1_000_000.0)
    } else if nanoseconds >= 1_000.0 {
        format!("{:.2} µs", nanoseconds / 1_000.0)
    } else {
        format!("{nanoseconds:.2} ns")
    }
}

fn gen_skill(check: bool) -> Result<()> {
    let repo_root = find_repo_root()?;
    let specs = load_tool_specs(&repo_root)?;

    let skill_path = repo_root.join("skill").join("SKILL.md");
    let tools_path = repo_root.join("docs").join("tools.md");

    let skill_existing = std::fs::read_to_string(&skill_path)
        .with_context(|| format!("failed to read {}", skill_path.display()))?;
    let tools_existing = match std::fs::read_to_string(&tools_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => format!("{START}{END}"),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", tools_path.display()))
        }
    };

    let skill_updated =
        replace_between_sentinels(&skill_existing, START, END, &render_skill_table(&specs))?;
    let tools_updated =
        replace_between_sentinels(&tools_existing, START, END, &render_tools_md(&specs))?;

    if check {
        let mut drift = false;
        if skill_updated != skill_existing {
            eprintln!("drift: {}", skill_path.display());
            drift = true;
        }
        if tools_updated != tools_existing {
            eprintln!("drift: {}", tools_path.display());
            drift = true;
        }
        if drift {
            bail!("generated docs are out of sync; run `just gen-skill`");
        }
        println!("skill/SKILL.md and docs/tools.md are in sync");
        return Ok(());
    }

    write_if_changed(&skill_path, &skill_existing, &skill_updated)?;
    if let Some(parent) = tools_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_if_changed(&tools_path, &tools_existing, &tools_updated)?;
    Ok(())
}

fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("failed to read current directory")?;
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join("src").is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("could not locate repository root");
        }
    }
}

fn load_tool_specs(repo_root: &Path) -> Result<Vec<ToolSpec>> {
    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--bin")
        .arg("lexa")
        .arg("--")
        .arg("dump-tools")
        .current_dir(repo_root)
        .output()
        .context("failed to run `cargo run --bin lexa -- dump-tools`")?;

    if !output.status.success() {
        bail!(
            "dump-tools failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout).context("failed to parse dump-tools JSON")
}

fn render_skill_table(specs: &[ToolSpec]) -> String {
    let mut out = String::new();
    out.push_str("| Tool | Use |\n");
    out.push_str("| --- | --- |\n");
    for spec in specs {
        writeln!(out, "| `{}` | {} |", spec.name, spec.summary).expect("write to string");
    }
    out.push('\n');
    out
}

fn render_tools_md(specs: &[ToolSpec]) -> String {
    let mut out = String::new();
    out.push_str("# MCP Tools Reference\n\n");
    out.push_str("> Generated from `TOOL_SPECS` in `src/mcp/tool_spec.rs`. ");
    out.push_str("Do not edit by hand; run `just gen-skill` to regenerate.\n\n");
    for spec in specs {
        writeln!(out, "## {}\n", spec.name).expect("write to string");
        writeln!(out, "**Summary:** {}\n", spec.summary).expect("write to string");
        writeln!(out, "**Description:** {}\n", spec.description).expect("write to string");
        let schema = serde_json::to_string_pretty(&spec.input_schema)
            .unwrap_or_else(|_| "<unserializable schema>".to_string());
        writeln!(out, "**Input schema:**\n\n```json\n{schema}\n```\n").expect("write to string");
    }
    out
}

fn replace_between_sentinels(
    source: &str,
    start_marker: &str,
    end_marker: &str,
    new_content: &str,
) -> Result<String> {
    let start = source
        .find(start_marker)
        .with_context(|| format!("missing sentinel: {start_marker}"))?;
    let end = source
        .find(end_marker)
        .with_context(|| format!("missing sentinel: {end_marker}"))?;
    if end < start + start_marker.len() {
        bail!("sentinel order invalid");
    }

    let mut out = String::with_capacity(source.len() + new_content.len());
    out.push_str(&source[..start + start_marker.len()]);
    out.push_str(new_content);
    out.push_str(&source[end..]);
    Ok(out)
}

fn write_if_changed(path: &Path, existing: &str, updated: &str) -> Result<()> {
    if existing == updated {
        println!("unchanged {}", path.display());
        return Ok(());
    }

    std::fs::write(path, updated).with_context(|| format!("failed to write {}", path.display()))?;
    println!("updated {}", path.display());
    Ok(())
}
