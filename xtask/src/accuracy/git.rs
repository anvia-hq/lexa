use super::model::{HistoricalTask, MutationPatch, RepositoryConfig, ToolCase, SCHEMA_VERSION};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

pub fn resolve_commit(repo: &Path, reference: &str) -> Result<String> {
    let output = git_output(
        repo,
        &["rev-parse", "--verify", &format!("{reference}^{{commit}}")],
    )?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub fn discover_historical_tasks(
    repo: &RepositoryConfig,
    pinned_commit: &str,
    count: usize,
) -> Result<Vec<HistoricalTask>> {
    let output = git_output(&repo.path, &["rev-list", "--no-merges", pinned_commit])?;
    let commits = String::from_utf8(output.stdout)?;
    let mut tasks = Vec::new();

    for commit in commits.lines().take(500) {
        if tasks.len() >= count {
            break;
        }
        let parents = git_text(&repo.path, &["show", "-s", "--format=%P", commit])?;
        let parents = parents.split_whitespace().collect::<Vec<_>>();
        if parents.len() != 1 {
            continue;
        }
        let parent = parents[0];
        let message = git_text(&repo.path, &["show", "-s", "--format=%B", commit])?;
        let Some(query) = benchmark_query(&message) else {
            continue;
        };
        let changed = changed_source_paths(&repo.path, parent, commit, &repo.source_extensions)?;
        if changed.is_empty() || changed.len() > 20 {
            continue;
        }

        tasks.push(HistoricalTask {
            schema_version: SCHEMA_VERSION,
            id: format!("{}:history:{}", repo.id, short_commit(commit)),
            repo_id: repo.id.clone(),
            source_commit: commit.to_string(),
            base_commit: parent.to_string(),
            query,
            relevant_paths: changed,
        });
    }

    if tasks.len() < count {
        bail!(
            "repository '{}' yielded only {} eligible historical tasks; requested {count}",
            repo.id,
            tasks.len()
        );
    }
    Ok(tasks)
}

fn benchmark_query(message: &str) -> Option<String> {
    let mut lines = message.lines();
    let subject = lines.next()?.trim();
    if subject.is_empty() || is_maintenance_subject(subject) {
        return None;
    }
    let body = lines
        .filter(|line| {
            let line = line.trim();
            !line.starts_with("Signed-off-by:")
                && !line.starts_with("Co-authored-by:")
                && !line.starts_with("Reviewed-by:")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    Some(if body.is_empty() {
        subject.to_string()
    } else {
        format!("{subject}\n\n{body}")
    })
}

fn is_maintenance_subject(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    lower.starts_with("chore")
        || lower.starts_with("release")
        || lower.starts_with("version package")
        || lower.starts_with("bump ")
        || lower.starts_with("merge ")
        || lower.contains("update lockfile")
}

fn changed_source_paths(
    repo: &Path,
    parent: &str,
    commit: &str,
    extensions: &[String],
) -> Result<Vec<String>> {
    let output = git_output(
        repo,
        &[
            "diff",
            "--name-status",
            "--find-renames",
            "--diff-filter=ACMDRT",
            parent,
            commit,
        ],
    )?;
    let text = String::from_utf8(output.stdout)?;
    let mut paths = BTreeSet::new();

    for line in text.lines() {
        let columns = line.split('\t').collect::<Vec<_>>();
        let Some(status) = columns.first().copied() else {
            continue;
        };
        let candidate = if matches!(status.chars().next(), Some('R' | 'M' | 'D' | 'T')) {
            columns.get(1).copied()
        } else {
            None
        };
        let Some(candidate) = candidate else {
            continue;
        };
        if is_source_path(candidate, extensions)
            && git_success(repo, &["cat-file", "-e", &format!("{parent}:{candidate}")])
        {
            paths.insert(candidate.replace('\\', "/"));
        }
    }
    Ok(paths.into_iter().collect())
}

pub fn generate_automatic_tool_cases(
    repo: &RepositoryConfig,
    commit: &str,
) -> Result<Vec<ToolCase>> {
    let files = tracked_source_files(&repo.path, commit, &repo.source_extensions)?;
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut cases = Vec::new();
    let selected = evenly_spaced(&files, 3);
    for path in selected {
        let query = fuzzy_path_query(path);
        cases.push(ToolCase {
            schema_version: SCHEMA_VERSION,
            id: format!("{}:path-search:{}", repo.id, sanitize_id(path)),
            repo_id: repo.id.clone(),
            commit: commit.to_string(),
            tool: "path-search".to_string(),
            args: vec![query, "--max-results".to_string(), "10".to_string()],
            expected_items: vec![path.to_string()],
            k: 10,
            reviewed: true,
        });
    }

    if let Some((directory, directory_files)) = select_bounded_directory(&files, 150) {
        let immediate = immediate_children(&directory, &directory_files);
        cases.push(ToolCase {
            schema_version: SCHEMA_VERSION,
            id: format!("{}:list:{}", repo.id, sanitize_id(&directory)),
            repo_id: repo.id.clone(),
            commit: commit.to_string(),
            tool: "list".to_string(),
            args: vec![directory.clone()],
            expected_items: immediate,
            k: 200,
            reviewed: true,
        });

        let extension = repo.source_extensions.first().cloned().unwrap_or_default();
        let extension = extension.trim_start_matches('.');
        let matching = directory_files
            .iter()
            .filter(|path| path.ends_with(&format!(".{extension}")))
            .cloned()
            .collect::<Vec<_>>();
        if !extension.is_empty() && !matching.is_empty() && matching.len() <= 200 {
            cases.push(ToolCase {
                schema_version: SCHEMA_VERSION,
                id: format!("{}:glob:{}", repo.id, sanitize_id(&directory)),
                repo_id: repo.id.clone(),
                commit: commit.to_string(),
                tool: "glob".to_string(),
                args: vec![format!("{directory}/**/*.{extension}")],
                expected_items: matching,
                k: 200,
                reviewed: true,
            });
        }

        if let Some(language) = repo.languages.first() {
            let language_extensions = extensions_for_language(language);
            let matching = directory_files
                .iter()
                .filter(|path| {
                    language_extensions
                        .iter()
                        .any(|extension| path.ends_with(&format!(".{extension}")))
                })
                .cloned()
                .collect::<Vec<_>>();
            if !matching.is_empty() && matching.len() <= 200 {
                cases.push(ToolCase {
                    schema_version: SCHEMA_VERSION,
                    id: format!("{}:files:{}", repo.id, sanitize_id(&directory)),
                    repo_id: repo.id.clone(),
                    commit: commit.to_string(),
                    tool: "files".to_string(),
                    args: vec![
                        directory.clone(),
                        "--language".to_string(),
                        language.clone(),
                        "--max-results".to_string(),
                        "200".to_string(),
                    ],
                    expected_items: matching,
                    k: 200,
                    reviewed: true,
                });
            }
        }
    }
    Ok(cases)
}

fn fuzzy_path_query(path: &str) -> String {
    let components = path.split('/').collect::<Vec<_>>();
    let start = components.len().saturating_sub(3);
    components[start..]
        .join("")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn extensions_for_language(language: &str) -> &'static [&'static str] {
    match language {
        "typescript" => &["ts", "tsx", "mts", "cts"],
        "javascript" => &["js", "jsx", "mjs", "cjs"],
        "rust" => &["rs"],
        "go" => &["go"],
        "python" => &["py"],
        _ => &[],
    }
}

fn tracked_source_files(repo: &Path, commit: &str, extensions: &[String]) -> Result<Vec<String>> {
    let output = git_output(repo, &["ls-tree", "-r", "--name-only", commit])?;
    let mut files = String::from_utf8(output.stdout)?
        .lines()
        .filter(|path| is_source_path(path, extensions))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn is_source_path(path: &str, extensions: &[String]) -> bool {
    extensions.iter().any(|extension| {
        let extension = extension.trim_start_matches('.');
        path.ends_with(&format!(".{extension}"))
    }) && !path.contains("/generated/")
        && !path.contains("/__generated__/")
        && !path.contains("/dist/")
        && !path.contains("/build/")
}

fn evenly_spaced<T>(items: &[T], count: usize) -> Vec<&T> {
    if items.len() <= count {
        return items.iter().collect();
    }
    (0..count)
        .map(|index| {
            let position = index * (items.len() - 1) / (count - 1);
            &items[position]
        })
        .collect()
}

fn select_bounded_directory(files: &[String], max_files: usize) -> Option<(String, Vec<String>)> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in files {
        let components = file.split('/').collect::<Vec<_>>();
        if components.len() < 2 {
            continue;
        }
        let depth = components.len().saturating_sub(1).min(2);
        let directory = components[..depth].join("/");
        groups.entry(directory).or_default().push(file.clone());
    }
    groups
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2 && paths.len() <= max_files)
        .max_by_key(|(_, paths)| paths.len())
}

fn immediate_children(directory: &str, files: &[String]) -> Vec<String> {
    let prefix = format!("{directory}/");
    files
        .iter()
        .filter_map(|path| path.strip_prefix(&prefix))
        .filter_map(|suffix| {
            let child = suffix.split('/').next()?;
            Some(if suffix.contains('/') {
                format!("{child}/")
            } else {
                child.to_string()
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

pub fn with_worktree<T>(
    repo: &Path,
    reference: &str,
    operation: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    let temp = tempfile::Builder::new()
        .prefix("lexa-accuracy-")
        .tempdir()
        .context("failed to create benchmark temporary directory")?;
    let worktree = temp.path().join("worktree");
    let output = Command::new("git")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree)
        .arg(reference)
        .current_dir(repo)
        .output()
        .with_context(|| format!("failed to create worktree for {}", repo.display()))?;
    if !output.status.success() {
        bail!(
            "git worktree add failed for {} at {reference}: {}",
            repo.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let result = operation(&worktree);
    let cleanup = remove_worktree(repo, &worktree, &temp);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => {
            Err(error.context(format!("worktree cleanup also failed: {cleanup_error:#}")))
        }
    }
}

fn remove_worktree(repo: &Path, worktree: &Path, _temp: &TempDir) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree)
        .current_dir(repo)
        .output()
        .with_context(|| format!("failed to remove worktree {}", worktree.display()))?;
    if !output.status.success() {
        bail!(
            "git worktree remove failed for {}: {}",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub fn apply_mutation(worktree: &Path, relative_path: &str, patch: &MutationPatch) -> Result<()> {
    let path = safe_join(worktree, relative_path)?;
    let source = fs::read_to_string(&path)
        .with_context(|| format!("failed to read mutation target {}", path.display()))?;
    let updated = match patch {
        MutationPatch::Replace { before, after } => replace_once(&source, before, after)?,
        MutationPatch::InsertBefore { anchor, content } => {
            replace_once(&source, anchor, &format!("{content}{anchor}"))?
        }
        MutationPatch::InsertAfter { anchor, content } => {
            replace_once(&source, anchor, &format!("{anchor}{content}"))?
        }
    };
    fs::write(&path, updated)
        .with_context(|| format!("failed to write mutation target {}", path.display()))
}

fn replace_once(source: &str, before: &str, after: &str) -> Result<String> {
    if before.is_empty() {
        bail!("mutation anchor may not be empty");
    }
    let matches = source.match_indices(before).collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!(
            "mutation precondition expected one exact match, found {}",
            matches.len()
        );
    }
    let index = matches[0].0;
    let mut updated = String::with_capacity(source.len() - before.len() + after.len());
    updated.push_str(&source[..index]);
    updated.push_str(after);
    updated.push_str(&source[index + before.len()..]);
    Ok(updated)
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("mutation path must stay inside the worktree: {relative:?}");
    }
    Ok(root.join(relative))
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_output(repo, args)?.stdout)?
        .trim()
        .to_string())
}

fn git_output(repo: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .with_context(|| format!("failed to run git {:?} in {}", args, repo.display()))?;
    if !output.status.success() {
        bail!(
            "git {:?} failed in {}: {}",
            args,
            repo.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

fn git_success(repo: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintenance_subjects_are_excluded() {
        assert!(benchmark_query("Version Packages\n").is_none());
        assert!(benchmark_query("chore: update lockfile\n").is_none());
        assert_eq!(
            benchmark_query("fix: resolve aliased import\n\nKeeps package paths stable.")
                .as_deref(),
            Some("fix: resolve aliased import\n\nKeeps package paths stable.")
        );
    }

    #[test]
    fn mutation_requires_one_exact_anchor() {
        assert!(replace_once("one two one", "one", "three").is_err());
        assert_eq!(
            replace_once("one two", "one", "three").unwrap(),
            "three two"
        );
    }

    #[test]
    fn immediate_children_include_files_and_directories_once() {
        let files = vec![
            "src/a.rs".to_string(),
            "src/nested/b.rs".to_string(),
            "src/nested/c.rs".to_string(),
        ];
        assert_eq!(
            immediate_children("src", &files),
            vec!["a.rs".to_string(), "nested/".to_string()]
        );
    }

    #[test]
    fn fuzzy_query_uses_distinguishing_path_components_without_separators() {
        assert_eq!(
            fuzzy_path_query("packages/vector-weaviate/vitest.config.ts"),
            "packagesvectorweaviatevitestconfigts"
        );
    }

    #[test]
    fn worktree_is_removed_when_operation_returns_an_error() {
        let repo = tempfile::tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success());
        for (key, value) in [
            ("user.name", "Lexa Benchmark"),
            ("user.email", "benchmark@example.invalid"),
        ] {
            assert!(Command::new("git")
                .args(["config", key, value])
                .current_dir(repo.path())
                .status()
                .unwrap()
                .success());
        }
        fs::write(repo.path().join("source.rs"), "fn main() {}\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "source.rs"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "--quiet", "-m", "initial"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success());

        let result: Result<()> = with_worktree(repo.path(), "HEAD", |_| {
            bail!("intentional benchmark failure")
        });

        assert!(result.is_err());
        let list = git_text(repo.path(), &["worktree", "list", "--porcelain"]).unwrap();
        assert!(!list.contains("lexa-accuracy-"));
    }
}
