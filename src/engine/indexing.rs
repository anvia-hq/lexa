use crate::index::trigram::{extract_trigrams, Trigram};
use crate::index::word::{prepare_word_index, PreparedWordIndex};
use crate::parser;
use crate::store::Op;
use crate::types::*;

use super::core::Engine;
use super::hash_content;
use super::shared::now_ms;

pub(super) struct PreparedIndexFile {
    path: String,
    content: String,
    modified_ms: u64,
    language: Language,
    line_count: u32,
    byte_size: u64,
    outline: FileOutline,
    trigrams: Vec<Trigram>,
    words: PreparedWordIndex,
}

pub(super) fn prepare_index_file(
    path: &str,
    content: String,
    modified_ms: u64,
) -> PreparedIndexFile {
    let language = detect_language(path);
    let line_count = content.lines().count().max(1) as u32;
    let byte_size = content.len() as u64;
    let outline = parser::parse_file(path, language, &content).unwrap_or_else(|| {
        let mut outline = FileOutline::new(path.to_string(), language);
        outline.line_count = line_count;
        outline.byte_size = byte_size;
        outline
    });
    let trigrams = extract_trigrams(&content);
    let words = prepare_word_index(&content);

    PreparedIndexFile {
        path: path.to_string(),
        content,
        modified_ms,
        language,
        line_count,
        byte_size,
        outline,
        trigrams,
        words,
    }
}

impl Engine {
    pub fn index_file(&mut self, path: &str, content: &str) {
        self.index_file_with_modified(path, content, now_ms());
    }

    pub fn index_file_with_modified(&mut self, path: &str, content: &str, modified_ms: u64) {
        self.index_file_with_op(path, content, modified_ms, Op::Snapshot, true);
    }

    pub(crate) fn index_file_with_modified_no_rebuild(
        &mut self,
        path: &str,
        content: &str,
        modified_ms: u64,
    ) {
        self.index_file_with_op(path, content, modified_ms, Op::Snapshot, false);
    }

    pub fn index_edited_file(&mut self, path: &str, content: &str, op: Op) {
        self.index_file_with_op(path, content, now_ms(), op, true);
    }

    pub(super) fn index_file_with_op(
        &mut self,
        path: &str,
        content: &str,
        modified_ms: u64,
        op: Op,
        rebuild_deps: bool,
    ) {
        let prepared = prepare_index_file(path, content.to_string(), modified_ms);
        self.index_prepared_file(prepared, op, rebuild_deps);
    }

    pub(super) fn index_prepared_file(
        &mut self,
        prepared: PreparedIndexFile,
        op: Op,
        rebuild_deps: bool,
    ) {
        let PreparedIndexFile {
            path,
            content,
            modified_ms,
            language,
            line_count,
            byte_size,
            outline,
            trigrams,
            words,
        } = prepared;
        let content_hash = hash_content(&content);

        self.symbol_index.index_file(&outline);
        self.trigram_index.index_prepared(&path, trigrams);
        self.word_index.index_prepared(&path, words);
        self.contents.insert(path.clone(), content);
        self.file_meta.insert(
            path.clone(),
            FileMeta {
                language,
                line_count,
                byte_size,
                symbol_count: outline.symbol_count() as u32,
                modified_ms,
                indexed: true,
            },
        );
        self.outlines.insert(path.clone(), outline);
        if rebuild_deps {
            self.rebuild_dep_graph();
        }
        match op {
            Op::Snapshot => {
                self.store.record_snapshot(&path, byte_size, content_hash);
            }
            Op::Replace | Op::Insert | Op::Delete | Op::Create => {
                self.store
                    .record_edit(&path, 0, op, content_hash, byte_size);
            }
            Op::Tombstone => {
                self.store.record_delete(&path, 0);
            }
        }
    }

    pub fn index_file_meta_only(&mut self, path: &str, byte_size: u64, modified_ms: u64) {
        self.index_file_meta_only_no_rebuild(path, byte_size, modified_ms);
        self.rebuild_dep_graph();
    }

    pub(crate) fn index_file_meta_only_no_dep_rebuild(
        &mut self,
        path: &str,
        byte_size: u64,
        modified_ms: u64,
    ) {
        self.index_file_meta_only_no_rebuild(path, byte_size, modified_ms);
    }

    pub(crate) fn update_file_metadata(&mut self, path: &str, byte_size: u64, modified_ms: u64) {
        if let Some(meta) = self.file_meta.get_mut(path) {
            meta.byte_size = byte_size;
            meta.modified_ms = modified_ms;
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        self.remove_file_no_dep_rebuild(path);
        self.rebuild_dep_graph();
    }

    pub(crate) fn remove_file_no_dep_rebuild(&mut self, path: &str) {
        self.outlines.remove(path);
        self.file_meta.remove(path);
        self.contents.remove(path);
        self.symbol_index.remove_file(path);
        self.trigram_index.remove_file(path);
        self.word_index.remove_file(path);
        self.dep_graph.remove(path);
        self.store.record_delete(path, 0);
    }
}
