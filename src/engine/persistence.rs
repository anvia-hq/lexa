use crate::index::symbol::SymbolIndex;
use crate::index::trigram::TrigramIndex;
use crate::index::word::WordIndex;
use crate::store::{Op, Store};
use crate::types::*;
use std::path::Path;

use super::core::Engine;
use super::hash_content;
use super::imports;

impl Engine {
    pub(super) fn rebuild_dep_graph(&mut self) {
        self.dep_graph.clear();
        for (path, outline) in &self.outlines {
            let resolution =
                imports::resolve_imports(path, &outline.imports, outline.language, &self.file_meta);
            let unresolved = resolution
                .unresolved
                .into_iter()
                .map(|import| unresolved_import_record(path, outline, import))
                .collect();
            self.dep_graph
                .set_resolution(path, resolution.deps, unresolved);
        }
    }

    pub(crate) fn rebuild_dep_graph_after_batch(&mut self) {
        self.rebuild_dep_graph();
    }

    pub fn to_snapshot_data(&self) -> EngineSnapshotData {
        EngineSnapshotData {
            outlines: self
                .outlines
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            file_meta: self
                .file_meta
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            contents: self
                .contents
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            forward_deps: self.dep_graph.forward_deps(),
        }
    }

    pub fn load_snapshot_data(&mut self, data: EngineSnapshotData) {
        self.outlines.clear();
        self.file_meta.clear();
        self.contents.clear();
        self.symbol_index = SymbolIndex::new();
        self.trigram_index = TrigramIndex::new();
        self.word_index = WordIndex::new();
        self.dep_graph.clear();
        self.store = Store::new();
        self.freshness_watermark_ns = None;

        for (path, outline) in data.outlines {
            self.symbol_index.index_file(&outline);
            self.outlines.insert(path, outline);
        }

        for (path, meta) in data.file_meta {
            self.file_meta.insert(path, meta);
        }

        for (path, content) in data.contents {
            self.trigram_index.index_file(&path, &content);
            self.word_index.index_file(&path, &content);
            self.contents.insert(path, content);
        }

        let _ = data.forward_deps;
        self.rebuild_dep_graph();
    }

    pub fn index_project(&mut self, root: impl AsRef<Path>) -> usize {
        self.freshness_watermark_ns = None;
        let root = root.as_ref();
        let files = crate::walker::walk_project_meta(root);
        let count = files.len();

        for file in &files {
            if file.indexable {
                let abs_path = root.join(&file.path);
                match std::fs::read_to_string(&abs_path) {
                    Ok(content) => self.index_file_with_op(
                        &file.path,
                        &content,
                        file.modified_ms,
                        Op::Snapshot,
                        false,
                    ),
                    Err(_) => self.index_file_meta_only_no_rebuild(
                        &file.path,
                        file.byte_size,
                        file.modified_ms,
                    ),
                }
            } else {
                self.index_file_meta_only_no_rebuild(&file.path, file.byte_size, file.modified_ms);
            }
        }
        self.rebuild_dep_graph();

        count
    }

    pub(super) fn index_file_meta_only_no_rebuild(
        &mut self,
        path: &str,
        byte_size: u64,
        modified_ms: u64,
    ) {
        self.outlines.remove(path);
        self.contents.remove(path);
        self.symbol_index.remove_file(path);
        self.trigram_index.remove_file(path);
        self.word_index.remove_file(path);
        self.file_meta.insert(
            path.to_string(),
            FileMeta {
                language: detect_language(path),
                line_count: 0,
                byte_size,
                symbol_count: 0,
                modified_ms,
                indexed: false,
            },
        );
        self.store.record_snapshot(
            path,
            byte_size,
            hash_content(&format!("{path}\0{byte_size}\0{modified_ms}")),
        );
    }
}

fn unresolved_import_record(path: &str, outline: &FileOutline, import: String) -> UnresolvedImport {
    let import_symbol = outline
        .symbols
        .iter()
        .find(|symbol| symbol.kind == SymbolKind::Import && symbol.name == import);
    UnresolvedImport {
        path: path.to_string(),
        import,
        line_start: import_symbol.map(|symbol| symbol.line_start),
        line_end: import_symbol.map(|symbol| symbol.line_end),
    }
}
