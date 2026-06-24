use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::engine::{hash_content, Engine};
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

    fn add(&mut self, other: RefreshSummary) {
        self.indexed += other.indexed;
        self.removed += other.removed;
    }
}

pub fn refresh_project(engine: &mut Engine, root: impl AsRef<Path>) -> Result<RefreshSummary> {
    let summary = refresh_project_no_rebuild(engine, root)?;
    if summary.changed() {
        engine.rebuild_dep_graph_after_batch();
    }
    Ok(summary)
}

fn refresh_project_no_rebuild(
    engine: &mut Engine,
    root: impl AsRef<Path>,
) -> Result<RefreshSummary> {
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

    let mut summary = RefreshSummary::default();

    for file in walked {
        let stale = indexed.get(&file.path).is_none_or(|meta| {
            meta.modified_ms != file.modified_ms || meta.byte_size != file.byte_size
        });
        if !stale {
            continue;
        }

        if file.indexable {
            let abs_path = root.join(&file.path);
            let content = std::fs::read_to_string(&abs_path)?;
            if indexed.contains_key(&file.path)
                && indexed_content_matches(engine, &file.path, &content)
            {
                continue;
            }
            engine.index_file_with_modified_no_rebuild(&file.path, &content, file.modified_ms);
        } else {
            engine.index_file_meta_only_no_dep_rebuild(
                &file.path,
                file.byte_size,
                file.modified_ms,
            );
        }
        summary.indexed += 1;
    }

    for path in indexed.keys() {
        if !walked_paths.contains(path) {
            engine.remove_file_no_dep_rebuild(path);
            summary.removed += 1;
        }
    }

    Ok(summary)
}

pub fn refresh_paths(
    engine: &mut Engine,
    root: impl AsRef<Path>,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<RefreshSummary> {
    let root = root.as_ref();
    let mut summary = RefreshSummary::default();
    let mut seen = HashSet::new();

    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }

        if path.is_dir() {
            summary.add(refresh_project_no_rebuild(engine, root)?);
            continue;
        }

        if path.exists() {
            if let Some(file) = walker::walk_single_file_meta(root, &path) {
                if file.indexable {
                    if let Some(walked) = walker::walk_single_file(root, &path) {
                        if indexed_content_matches(engine, &walked.path, &walked.content) {
                            continue;
                        }
                        engine.index_file_with_modified_no_rebuild(
                            &walked.path,
                            &walked.content,
                            walked.modified_ms,
                        );
                    } else {
                        engine.index_file_meta_only_no_dep_rebuild(
                            &file.path,
                            file.byte_size,
                            file.modified_ms,
                        );
                    }
                } else {
                    engine.index_file_meta_only_no_dep_rebuild(
                        &file.path,
                        file.byte_size,
                        file.modified_ms,
                    );
                }
                summary.indexed += 1;
            } else if let Some(relative) = walker::relative_path(root, &path) {
                remove_indexed_path(engine, &relative, &mut summary);
            }
            continue;
        }

        if let Some(relative) = walker::relative_path(root, &path) {
            remove_indexed_path(engine, &relative, &mut summary);
        }
    }

    if summary.changed() {
        engine.rebuild_dep_graph_after_batch();
    }

    Ok(summary)
}

fn indexed_content_matches(engine: &Engine, path: &str, content: &str) -> bool {
    engine
        .read_file_rich(path, None, None, false, None)
        .is_some_and(|result| result.hash == hash_content(content))
}

fn remove_indexed_path(engine: &mut Engine, relative: &str, summary: &mut RefreshSummary) {
    let prefix = format!("{relative}/");
    let paths = engine
        .file_map()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path == relative || path.starts_with(&prefix))
        .collect::<Vec<_>>();

    for path in paths {
        engine.remove_file_no_dep_rebuild(&path);
        summary.removed += 1;
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
            "import { newValue } from './new_dep';\nexport const app = newValue;\n",
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
