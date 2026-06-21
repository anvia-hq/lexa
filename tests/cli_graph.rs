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
