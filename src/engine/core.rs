use crate::index::symbol::SymbolIndex;
use crate::index::trigram::TrigramIndex;
use crate::index::word::WordIndex;
use crate::store::Store;
use crate::types::*;
use hashbrown::HashMap;
use serde::Serialize;

use super::dep_graph::DepGraph;

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub max_results: usize,
    pub regex: bool,
    pub scope: bool,
    pub compact: bool,
    pub paths_only: bool,
    pub path_glob: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FileFilterOptions {
    pub path_prefix: Option<String>,
    pub path_glob: Option<String>,
    pub language: Option<String>,
    pub min_lines: Option<u32>,
    pub max_lines: Option<u32>,
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct WordSearchOptions {
    pub path_prefix: Option<String>,
    pub path_glob: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WordSearchResult {
    pub path: String,
    pub line_num: u32,
    pub line_text: String,
    pub kind: String,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RichSearchResult {
    pub path: String,
    pub line_num: u32,
    pub line_text: String,
    pub scope: Option<Symbol>,
}

#[derive(Debug, Clone)]
pub struct ReadFileResult {
    pub content: String,
    pub hash: u64,
    pub unchanged: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextDetails {
    pub task: String,
    pub keywords: Vec<String>,
    pub max_results: usize,
    pub confidence: String,
    pub note: Option<String>,
    pub suggested_next_steps: Vec<String>,
    pub relevant_symbols: Vec<ContextSymbol>,
    pub snippets: Vec<SearchResult>,
}

#[derive(Debug, Clone, Default)]
pub struct ContextOptions {
    pub max_results: usize,
    pub path_prefix: Option<String>,
    pub path_glob: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextSymbol {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub line_start: u32,
    pub line_end: u32,
    pub detail: Option<String>,
    pub content_line_start: u32,
    pub content_line_end: u32,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolSearchResult {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub line_start: u32,
    pub line_end: u32,
    pub detail: Option<String>,
    pub score: f32,
    pub raw_score: i32,
}

pub struct Engine {
    pub(super) outlines: HashMap<String, FileOutline>,
    pub(super) file_meta: HashMap<String, FileMeta>,
    pub(super) contents: HashMap<String, String>,
    pub(super) symbol_index: SymbolIndex,
    pub(super) trigram_index: TrigramIndex,
    pub(super) word_index: WordIndex,
    pub(super) dep_graph: DepGraph,
    pub(super) store: Store,
    pub(super) freshness_watermark_ns: Option<u128>,
}

impl Engine {
    pub fn new(_cache_capacity: u32) -> Self {
        Self {
            outlines: HashMap::new(),
            file_meta: HashMap::new(),
            contents: HashMap::new(),
            symbol_index: SymbolIndex::new(),
            trigram_index: TrigramIndex::new(),
            word_index: WordIndex::new(),
            dep_graph: DepGraph::new(),
            store: Store::new(),
            freshness_watermark_ns: None,
        }
    }

    pub(crate) fn set_freshness_watermark(&mut self, watermark_ns: Option<u128>) {
        self.freshness_watermark_ns = watermark_ns;
    }

    pub(crate) fn content_unchanged_since_snapshot(&self, change_ns: Option<u128>) -> bool {
        matches!(
            (change_ns, self.freshness_watermark_ns),
            (Some(change), Some(watermark)) if change <= watermark
        )
    }
}
