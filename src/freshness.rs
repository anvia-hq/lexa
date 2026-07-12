use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::engine::Engine;
use crate::walker;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RefreshSummary {
    pub indexed: usize,
    pub removed: usize,
}

impl RefreshSummary {
    pub fn changed(self) -> bool {
        self.indexed > 0 || self.removed > 0
    }
}

#[derive(Default)]
struct RefreshWork {
    summary: RefreshSummary,
    dependencies_changed: bool,
}

impl RefreshWork {
    fn add(&mut self, other: RefreshWork) {
        self.summary.indexed += other.summary.indexed;
        self.summary.removed += other.summary.removed;
        self.dependencies_changed |= other.dependencies_changed;
    }
}

pub fn refresh_project(engine: &mut Engine, root: impl AsRef<Path>) -> Result<RefreshSummary> {
    let work = refresh_project_no_rebuild(engine, root)?;
    if work.dependencies_changed {
        engine.rebuild_dep_graph_after_batch();
    }
    Ok(work.summary)
}

fn refresh_project_no_rebuild(engine: &mut Engine, root: impl AsRef<Path>) -> Result<RefreshWork> {
    let root = root.as_ref();
    let walked = walker::walk_project_meta(root);
    let indexed = engine
        .file_map()
        .into_iter()
        .collect::<HashMap<String, crate::types::FileMeta>>();
    let walked_paths = walked
        .iter()
        .map(|file| file.path.clone())
        .collect::<HashSet<_>>();

    let mut work = RefreshWork::default();

    for file in walked {
        refresh_walked_file(engine, root, &file, indexed.get(&file.path), &mut work)?;
    }

    for path in indexed.keys() {
        if !walked_paths.contains(path) {
            engine.remove_file_no_dep_rebuild(path);
            work.summary.removed += 1;
            work.dependencies_changed = true;
        }
    }

    Ok(work)
}

pub fn refresh_paths(
    engine: &mut Engine,
    root: impl AsRef<Path>,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<RefreshSummary> {
    let root = root.as_ref();
    let mut work = RefreshWork::default();
    let mut seen = HashSet::new();

    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }

        if path.is_dir() {
            work.add(refresh_project_no_rebuild(engine, root)?);
            continue;
        }

        if path.exists() {
            if let Some(file) = walker::walk_single_file_meta(root, &path) {
                let indexed = engine
                    .file_map()
                    .into_iter()
                    .find_map(|(path, meta)| (path == file.path).then_some(meta));
                refresh_walked_file(engine, root, &file, indexed.as_ref(), &mut work)?;
            } else if let Some(relative) = walker::relative_path(root, &path) {
                remove_indexed_path(engine, &relative, &mut work);
            }
            continue;
        }

        if let Some(relative) = walker::relative_path(root, &path) {
            remove_indexed_path(engine, &relative, &mut work);
        }
    }

    if work.dependencies_changed {
        engine.rebuild_dep_graph_after_batch();
    }

    Ok(work.summary)
}

fn refresh_walked_file(
    engine: &mut Engine,
    root: &Path,
    file: &walker::WalkedFileMeta,
    indexed: Option<&crate::types::FileMeta>,
    work: &mut RefreshWork,
) -> Result<()> {
    if file.indexable {
        let metadata_matches = indexed.is_some_and(|meta| {
            meta.modified_ms == file.modified_ms && meta.byte_size == file.byte_size && meta.indexed
        });
        if metadata_matches && engine.content_unchanged_since_snapshot(file.change_ns) {
            return Ok(());
        }

        let path = root.join(&file.path);
        let walked = walker::walk_single_file(root, &path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let content = walked.content;
        debug_assert_eq!(walked.path, file.path);
        if engine.content(&file.path) == Some(content.as_str()) && indexed.is_some() {
            if !metadata_matches {
                engine.update_file_metadata(&file.path, content.len() as u64, walked.modified_ms);
                work.summary.indexed += 1;
            }
            return Ok(());
        }

        engine.index_file_with_modified_no_rebuild(&file.path, &content, walked.modified_ms);
        work.summary.indexed += 1;
        work.dependencies_changed = true;
        return Ok(());
    }

    let metadata_matches = indexed.is_some_and(|meta| {
        meta.modified_ms == file.modified_ms && meta.byte_size == file.byte_size && !meta.indexed
    });

    if metadata_matches {
        return Ok(());
    }

    engine.index_file_meta_only_no_dep_rebuild(&file.path, file.byte_size, file.modified_ms);
    work.summary.indexed += 1;
    work.dependencies_changed |= indexed.is_none_or(|meta| meta.indexed);
    Ok(())
}

fn remove_indexed_path(engine: &mut Engine, relative: &str, work: &mut RefreshWork) {
    let prefix = format!("{relative}/");
    let paths = engine
        .file_map()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path == relative || path.starts_with(&prefix))
        .collect::<Vec<_>>();

    for path in paths {
        engine.remove_file_no_dep_rebuild(&path);
        work.summary.removed += 1;
        work.dependencies_changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn refresh_project_indexes_changed_files_only() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("src.rs");
        std::fs::write(&path, "fn old() {}\n").unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);
        std::fs::write(&path, "fn new_name() {}\n").unwrap();

        let summary = refresh_project(&mut engine, root).unwrap();

        assert_eq!(summary.indexed, 1);
        assert_eq!(summary.removed, 0);
        assert!(!engine.find_symbol("new_name").is_empty());
        assert!(engine.find_symbol("old").is_empty());
    }

    #[test]
    fn refresh_project_detects_same_size_content_with_restored_mtime() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("src.rs");
        std::fs::write(&path, "fn old() {}\n").unwrap();
        let original_modified = path.metadata().unwrap().modified().unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);

        std::fs::write(&path, "fn new() {}\n").unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(original_modified))
            .unwrap();

        let summary = refresh_project(&mut engine, root).unwrap();

        assert_eq!(summary.indexed, 1);
        assert!(!engine.find_symbol("new").is_empty());
        assert!(engine.find_symbol("old").is_empty());
    }

    #[test]
    fn snapshot_fast_path_still_detects_restored_mtime_content_changes() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let root = project.as_path();
        let source_path = root.join("src.rs");
        let snapshot_path = dir.path().join("graph.lexa");
        std::fs::write(&source_path, "fn old() {}\n").unwrap();
        let original_modified = source_path.metadata().unwrap().modified().unwrap();

        let mut indexed = Engine::new(16);
        indexed.index_project(root);
        crate::snapshot::write_snapshot(&indexed, &snapshot_path).unwrap();

        let mut loaded = Engine::new(16);
        crate::snapshot::load_snapshot_into_engine(&mut loaded, &snapshot_path).unwrap();
        std::fs::write(&source_path, "fn new() {}\n").unwrap();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&source_path)
            .unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(original_modified))
            .unwrap();

        let summary = refresh_project(&mut loaded, root).unwrap();

        assert_eq!(summary.indexed, 1);
        assert!(!loaded.find_symbol("new").is_empty());
        assert!(loaded.find_symbol("old").is_empty());
    }

    #[test]
    fn refresh_project_persists_metadata_only_changes_once() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("src.rs");
        std::fs::write(&path, "fn same() {}\n").unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);
        let newer = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(newer))
            .unwrap();

        let first = refresh_project(&mut engine, root).unwrap();
        let second = refresh_project(&mut engine, root).unwrap();

        assert_eq!(first.indexed, 1);
        assert_eq!(second.indexed, 0);
        let modified_ms = engine
            .file_map()
            .into_iter()
            .find_map(|(candidate, meta)| (candidate == "src.rs").then_some(meta.modified_ms))
            .unwrap();
        assert!(modified_ms > 0);
    }

    #[test]
    fn refresh_project_indexes_new_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let mut engine = Engine::new(16);
        engine.index_project(root);

        std::fs::write(root.join("added.rs"), "fn added() {}\n").unwrap();

        let summary = refresh_project(&mut engine, root).unwrap();

        assert_eq!(summary.indexed, 1);
        assert_eq!(summary.removed, 0);
        assert!(!engine.find_symbol("added").is_empty());
    }

    #[test]
    fn refresh_project_removes_deleted_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("gone.rs");
        std::fs::write(&path, "fn gone() {}\n").unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);
        std::fs::remove_file(&path).unwrap();

        let summary = refresh_project(&mut engine, root).unwrap();

        assert_eq!(summary.indexed, 0);
        assert_eq!(summary.removed, 1);
        assert!(engine.find_symbol("gone").is_empty());
        assert!(engine.file_map().is_empty());
    }

    #[test]
    fn refresh_project_rebuilds_dependency_graph_after_mixed_batch() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/app.ts"),
            "import { oldValue } from './old_dep';\nexport const app = oldValue;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/old_dep.ts"),
            "export const oldValue = 'old';\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/remove_me.ts"),
            "export const removeMe = 1;\n",
        )
        .unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);
        assert_eq!(
            engine.get_depends_on("src/app.ts"),
            vec!["src/old_dep.ts".to_string()]
        );

        std::fs::write(
            root.join("src/app.ts"),
            "import { newValue } from './new_dep';\nexport const app = newValue;\nexport const extra = app;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/new_dep.ts"),
            "export const newValue = 'new';\n",
        )
        .unwrap();
        std::fs::remove_file(root.join("src/remove_me.ts")).unwrap();

        let summary = refresh_project(&mut engine, root).unwrap();

        assert_eq!(summary.indexed, 2);
        assert_eq!(summary.removed, 1);
        assert_eq!(
            engine.get_depends_on("src/app.ts"),
            vec!["src/new_dep.ts".to_string()]
        );
        assert_eq!(
            engine.get_imported_by("src/new_dep.ts"),
            vec!["src/app.ts".to_string()]
        );
        assert!(engine.get_imported_by("src/old_dep.ts").is_empty());
        assert!(!engine
            .file_map()
            .iter()
            .any(|(path, _)| path == "src/remove_me.ts"));
    }

    #[test]
    fn refresh_paths_updates_single_file() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("event.rs");
        std::fs::write(&path, "fn before() {}\n").unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);
        std::fs::write(&path, "fn after() {}\n").unwrap();

        let summary = refresh_paths(&mut engine, root, vec![path]).unwrap();

        assert_eq!(summary.indexed, 1);
        assert_eq!(summary.removed, 0);
        assert!(!engine.find_symbol("after").is_empty());
        assert!(engine.find_symbol("before").is_empty());
    }

    #[test]
    fn refresh_paths_skips_matching_indexed_content() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let path = root.join("same.rs");
        std::fs::write(&path, "fn same() {}\n").unwrap();

        let mut engine = Engine::new(16);
        engine.index_project(root);

        let summary = refresh_paths(&mut engine, root, vec![path]).unwrap();

        assert_eq!(summary.indexed, 0);
        assert_eq!(summary.removed, 0);
    }

    #[test]
    fn refresh_paths_ignores_hidden_internal_paths() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".lexa")).unwrap();
        let log_path = root.join(".lexa/mcp.log");
        std::fs::write(&log_path, "diagnostic line\n").unwrap();

        let mut engine = Engine::new(16);
        let summary = refresh_paths(&mut engine, root, vec![log_path]).unwrap();

        assert_eq!(summary.indexed, 0);
        assert_eq!(summary.removed, 0);
        assert!(engine.file_map().is_empty());
    }
}
