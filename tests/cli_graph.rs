#![allow(clippy::unwrap_used)]

use std::process::Command;

fn lexa() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lexa"))
}

#[test]
fn index_writes_default_graph_under_indexed_project_root() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let caller = temp.path().join("caller");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&caller).unwrap();
    std::fs::write(project.join("a.rs"), "fn one() {}\n").unwrap();

    let output = lexa()
        .current_dir(&caller)
        .arg("index")
        .arg(&project)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join(".lexa/graph.lexa").exists());
    assert!(!caller.join(".lexa/graph.lexa").exists());
}

#[test]
fn index_respects_explicit_graph_override() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let custom_graph = temp.path().join("custom.lexa");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("a.rs"), "fn one() {}\n").unwrap();

    let output = lexa()
        .current_dir(temp.path())
        .arg("--graph")
        .arg(&custom_graph)
        .arg("index")
        .arg(&project)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(custom_graph.exists());
    assert!(!project.join(".lexa/graph.lexa").exists());
}

#[test]
fn no_graph_patch_changes_file_without_persisting_graph() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let path = project.join("a.rs");
    std::fs::write(&path, "fn one() {}\n").unwrap();

    let output = lexa()
        .current_dir(project)
        .args([
            "--no-graph",
            "patch",
            "a.rs",
            "replace",
            "-L",
            "1",
            "--content",
            "fn changed() {}",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fn changed() {}\n");
    assert!(!project.join(".lexa/graph.lexa").exists());
}

#[test]
fn no_graph_create_writes_file_without_persisting_graph() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();

    let output = lexa()
        .current_dir(project)
        .args([
            "--no-graph",
            "create",
            "new.rs",
            "--content",
            "fn created() {}\n",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(project.join("new.rs")).unwrap(),
        "fn created() {}\n"
    );
    assert!(!project.join(".lexa/graph.lexa").exists());
}

#[test]
fn persisted_patch_requires_existing_graph_before_mutating_file() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let path = project.join("a.rs");
    std::fs::write(&path, "fn one() {}\n").unwrap();

    let output = lexa()
        .current_dir(project)
        .args([
            "patch",
            "a.rs",
            "replace",
            "-L",
            "1",
            "--content",
            "fn changed() {}",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fn one() {}\n");
    assert!(!project.join(".lexa/graph.lexa").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no graph file found"));
}

#[test]
fn persisted_create_requires_existing_graph_before_creating_file() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();

    let output = lexa()
        .current_dir(project)
        .args(["create", "new.rs", "--content", "fn created() {}\n"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!project.join("new.rs").exists());
    assert!(!project.join(".lexa/graph.lexa").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no graph file found"));
}

#[test]
fn audit_requires_indexed_files() {
    let temp = tempfile::tempdir().unwrap();

    let output = lexa()
        .current_dir(temp.path())
        .arg("audit")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no files indexed"));
}

#[test]
fn cli_accepts_agent_friendly_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    std::fs::write(project.join("a.rs"), "fn one() {}\nfn two() {}\n").unwrap();

    assert!(lexa()
        .current_dir(project)
        .arg("index")
        .arg(".")
        .output()
        .unwrap()
        .status
        .success());

    let read = lexa()
        .current_dir(project)
        .args(["read", "a.rs", "--line-start", "2", "--line-end", "2"])
        .output()
        .unwrap();
    assert!(read.status.success());
    assert_eq!(String::from_utf8_lossy(&read.stdout), "fn two() {}");

    let search = lexa()
        .current_dir(project)
        .args(["text-search", "--query", "fn", "--max-results", "1"])
        .output()
        .unwrap();
    assert!(search.status.success());
    assert!(String::from_utf8_lossy(&search.stdout).contains("1 results"));

    let path = lexa()
        .current_dir(project)
        .args(["path-search", "--query", "a", "--max-results", "1"])
        .output()
        .unwrap();
    assert!(path.status.success());
    assert!(String::from_utf8_lossy(&path.stdout).contains("a.rs"));
}

#[test]
fn cli_rejects_mixed_line_range_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    std::fs::write(project.join("a.rs"), "one\n").unwrap();

    let output = lexa()
        .current_dir(project)
        .args([
            "--no-graph",
            "read",
            "a.rs",
            "--line-range",
            "1",
            "--line-start",
            "1",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("use either --line-range or --line-start/--line-end"));
}

#[test]
fn cli_auto_refreshes_stale_graph_before_read_and_search() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let path = project.join("a.rs");
    std::fs::write(&path, "fn old() {}\n").unwrap();

    assert!(lexa()
        .current_dir(project)
        .arg("index")
        .arg(".")
        .output()
        .unwrap()
        .status
        .success());

    std::fs::write(&path, "fn fresh() {}\n").unwrap();

    let read = lexa()
        .current_dir(project)
        .args(["read", "a.rs"])
        .output()
        .unwrap();
    assert!(read.status.success());
    assert!(String::from_utf8_lossy(&read.stdout).contains("fresh"));
    let read_stderr = String::from_utf8_lossy(&read.stderr);
    assert!(read_stderr.contains("Checking graph freshness"));
    assert!(read_stderr.contains("Refreshed graph"));

    let search = lexa()
        .current_dir(project)
        .args(["text-search", "fresh"])
        .output()
        .unwrap();
    assert!(search.status.success());
    assert!(String::from_utf8_lossy(&search.stdout).contains("fresh"));

    std::fs::remove_file(&path).unwrap();
    let missing = lexa()
        .current_dir(project)
        .args(["read", "a.rs"])
        .output()
        .unwrap();
    assert!(missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stdout).contains("File not found"));
}

#[test]
fn cli_patch_compact_preview_and_success_output_are_focused() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let path = project.join("a.rs");
    std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();

    assert!(lexa()
        .current_dir(project)
        .arg("index")
        .arg(".")
        .output()
        .unwrap()
        .status
        .success());

    let preview = lexa()
        .current_dir(project)
        .args([
            "patch",
            "a.rs",
            "insert",
            "--after",
            "2",
            "--content",
            "inserted",
            "--dry-run",
            "--preview",
            "compact",
        ])
        .output()
        .unwrap();
    assert!(preview.status.success());
    let stdout = String::from_utf8_lossy(&preview.stdout);
    assert!(stdout.contains("+    3: inserted"));
    assert!(!stdout.contains("-    3: three"));

    let changed = lexa()
        .current_dir(project)
        .args([
            "patch",
            "a.rs",
            "insert",
            "--after",
            "2",
            "--content",
            "inserted",
        ])
        .output()
        .unwrap();
    assert!(changed.status.success());
    let stdout = String::from_utf8_lossy(&changed.stdout);
    assert!(stdout.contains("edit applied to a.rs: +1 -0 lines (6 total)"));
}

#[test]
fn cli_patch_reports_content_change_when_line_counts_do_not_change() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let path = project.join("a.rs");
    std::fs::write(&path, "one\ntwo").unwrap();

    assert!(lexa()
        .current_dir(project)
        .arg("index")
        .arg(".")
        .output()
        .unwrap()
        .status
        .success());

    let changed = lexa()
        .current_dir(project)
        .args([
            "patch",
            "a.rs",
            "insert",
            "--after",
            "99",
            "--content",
            "\n",
        ])
        .output()
        .unwrap();

    assert!(changed.status.success());
    assert!(String::from_utf8_lossy(&changed.stdout)
        .contains("content changed without line-count change (2 total)"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
}

#[test]
fn cli_patch_supports_replace_text_and_anchor_modes() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let path = project.join("a.rs");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();

    assert!(lexa()
        .current_dir(project)
        .arg("index")
        .arg(".")
        .output()
        .unwrap()
        .status
        .success());

    let replace = lexa()
        .current_dir(project)
        .args(["patch", "a.rs", "--replace-text", "two", "--content", "TWO"])
        .output()
        .unwrap();
    assert!(replace.status.success());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\nTWO\nthree\n");

    let anchor = lexa()
        .current_dir(project)
        .args([
            "patch",
            "a.rs",
            "--anchor",
            "TWO",
            "--placement",
            "after",
            "--content",
            "inserted",
        ])
        .output()
        .unwrap();
    assert!(anchor.status.success());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "one\nTWO\ninserted\nthree\n"
    );

    std::fs::write(&path, "same\nsame\n").unwrap();
    let ambiguous = lexa()
        .current_dir(project)
        .args([
            "patch",
            "a.rs",
            "--replace-text",
            "same",
            "--content",
            "changed",
        ])
        .output()
        .unwrap();
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("matched multiple locations"));
}
