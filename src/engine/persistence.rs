use crate::index::symbol::SymbolIndex;
use crate::index::trigram::TrigramIndex;
use crate::index::word::WordIndex;
use crate::store::{Op, Store};
use crate::types::*;
use crate::walker::WalkedFileMeta;
use rayon::prelude::*;
use std::path::Path;

use super::core::Engine;
use super::hash_content;
use super::imports;
use super::indexing::{prepare_index_file, PreparedIndexFile};

const PARALLEL_INDEX_THRESHOLD: usize = 64;

enum PreparedProjectFile {
    Indexed(PreparedIndexFile),
    Metadata {
        path: String,
        byte_size: u64,
        modified_ms: u64,
    },
}

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
        let mut outlines = self
            .outlines
            .iter()
            .map(|(path, outline)| (path.clone(), outline.clone()))
            .collect::<Vec<_>>();
        outlines.sort_by(|left, right| left.0.cmp(&right.0));

        let mut file_meta = self
            .file_meta
            .iter()
            .map(|(path, meta)| (path.clone(), meta.clone()))
            .collect::<Vec<_>>();
        file_meta.sort_by(|left, right| left.0.cmp(&right.0));

        let mut contents = self
            .contents
            .iter()
            .map(|(path, content)| (path.clone(), content.clone()))
            .collect::<Vec<_>>();
        contents.sort_by(|left, right| left.0.cmp(&right.0));

        EngineSnapshotData {
            outlines,
            file_meta,
            contents,
            forward_deps: self.dep_graph.forward_deps(),
            unresolved_imports: self.dep_graph.unresolved_imports_by_path(),
            indexes: Some(EngineIndexSnapshot {
                symbols: self.symbol_index.snapshot(),
                trigrams: self.trigram_index.snapshot(),
                words: self.word_index.snapshot(),
            }),
        }
    }

    pub fn load_snapshot_data(&mut self, data: EngineSnapshotData) {
        let EngineSnapshotData {
            outlines,
            file_meta,
            contents,
            forward_deps,
            unresolved_imports,
            indexes,
        } = data;
        self.outlines.clear();
        self.file_meta.clear();
        self.contents.clear();
        self.symbol_index = SymbolIndex::new();
        self.trigram_index = TrigramIndex::new();
        self.word_index = WordIndex::new();
        self.dep_graph.clear();
        self.store = Store::new();
        self.freshness_watermark_ns = None;

        for (path, outline) in outlines {
            self.outlines.insert(path, outline);
        }

        for (path, meta) in file_meta {
            self.file_meta.insert(path, meta);
        }

        for (path, content) in contents {
            self.contents.insert(path, content);
        }

        let hydrated_snapshot = indexes.is_some();
        if let Some(indexes) = indexes {
            let symbols = SymbolIndex::from_snapshot(indexes.symbols);
            let trigrams = TrigramIndex::from_snapshot(indexes.trigrams);
            let words = WordIndex::from_snapshot(indexes.words);
            if let (Some(trigrams), Some(words)) = (trigrams, words) {
                self.symbol_index = symbols;
                self.trigram_index = trigrams;
                self.word_index = words;
            } else {
                self.rebuild_search_indexes();
            }
        } else {
            self.rebuild_search_indexes();
        }

        if hydrated_snapshot {
            if let Some(dep_graph) =
                super::dep_graph::DepGraph::from_snapshot(forward_deps, unresolved_imports)
            {
                self.dep_graph = dep_graph;
            } else {
                self.rebuild_dep_graph();
            }
        } else {
            self.rebuild_dep_graph();
        }
    }

    pub fn index_project(&mut self, root: impl AsRef<Path>) -> usize {
        self.freshness_watermark_ns = None;
        let root = root.as_ref();
        let files = crate::walker::walk_project_meta(root);
        let count = files.len();

        let prepare = |file: &WalkedFileMeta| prepare_project_file(root, file);
        let prepared = if files.len() >= PARALLEL_INDEX_THRESHOLD {
            files.par_iter().map(prepare).collect::<Vec<_>>()
        } else {
            files.iter().map(prepare).collect::<Vec<_>>()
        };

        for file in prepared {
            match file {
                PreparedProjectFile::Indexed(file) => {
                    self.index_prepared_file(file, Op::Snapshot, false);
                }
                PreparedProjectFile::Metadata {
                    path,
                    byte_size,
                    modified_ms,
                } => {
                    self.index_file_meta_only_no_rebuild(&path, byte_size, modified_ms);
                }
            }
        }
        self.rebuild_dep_graph();

        count
    }

    fn rebuild_search_indexes(&mut self) {
        self.symbol_index = SymbolIndex::new();
        self.trigram_index = TrigramIndex::new();
        self.word_index = WordIndex::new();

        for outline in self.outlines.values() {
            self.symbol_index.index_file(outline);
        }
        for (path, content) in &self.contents {
            self.trigram_index.index_file(path, content);
            self.word_index.index_file(path, content);
        }
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

fn prepare_project_file(root: &Path, file: &WalkedFileMeta) -> PreparedProjectFile {
    if file.indexable {
        let abs_path = root.join(&file.path);
        if let Ok(content) = std::fs::read_to_string(abs_path) {
            return PreparedProjectFile::Indexed(prepare_index_file(
                &file.path,
                content,
                file.modified_ms,
            ));
        }
    }

    PreparedProjectFile::Metadata {
        path: file.path.clone(),
        byte_size: file.byte_size,
        modified_ms: file.modified_ms,
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
