use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditScopeReport {
    Project,
    GitSince {
        base: String,
        changed_files: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditScope {
    Project,
    GitSince {
        base: String,
        changed_files: Vec<String>,
    },
}

impl AuditScope {
    pub(crate) fn report(&self) -> AuditScopeReport {
        match self {
            Self::Project => AuditScopeReport::Project,
            Self::GitSince {
                base,
                changed_files,
            } => AuditScopeReport::GitSince {
                base: base.clone(),
                changed_files: changed_files.clone(),
            },
        }
    }
}

pub fn changed_files_since(root: &Path, base: &str) -> Result<Vec<String>> {
    let prefix = git_prefix(root).unwrap_or_default();
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--name-only")
        .arg("--diff-filter=ACMRT")
        .arg(format!("{base}...HEAD"))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git diff failed for base '{base}': {stderr}");
    }

    let mut files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| normalize_git_changed_path(line.trim(), &prefix))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    Ok(files)
}

fn git_prefix(root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--show-prefix")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git rev-parse failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace('\\', "/"))
}

pub(crate) fn normalize_git_changed_path(path: &str, prefix: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if prefix.is_empty() {
        return Some(path);
    }
    path.strip_prefix(prefix).map(ToString::to_string)
}
