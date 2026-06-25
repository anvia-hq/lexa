use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const START: &str = "<!-- TOOLS START -->\n";
const END: &str = "<!-- TOOLS END -->\n";

#[derive(Debug, Deserialize)]
struct ToolSpec {
    name: String,
    summary: String,
    description: String,
    input_schema: Value,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        bail!("usage: xtask gen-skill [--check]");
    };

    match command.as_str() {
        "gen-skill" => match args.collect::<Vec<_>>().as_slice() {
            [] => gen_skill(false),
            [flag] if flag == "--check" => gen_skill(true),
            _ => bail!("usage: xtask gen-skill [--check]"),
        },
        other => bail!("unknown xtask command: {other}"),
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
