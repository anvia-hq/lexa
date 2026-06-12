use crate::cache::ContentCache;
use crate::glob::match_glob;
use crate::index::symbol::SymbolIndex;
use crate::index::trigram::TrigramIndex;
use crate::index::word::WordIndex;
use crate::parser;
use crate::snapshot;
use crate::store::{Op, Store};
use crate::types::*;
use hashbrown::{HashMap, HashSet};
use regex::Regex;
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CONTEXT_SYMBOL_LINES: u32 = 120;
const CONTEXT_EXACT_SYMBOL_BONUS: i32 = 500;
const CONTEXT_CASE_INSENSITIVE_SYMBOL_BONUS: i32 = 450;
const CONTEXT_INDEXED_SYMBOL_SOURCE_BONUS: i32 = 500;
const CONTEXT_OUTLINE_SYMBOL_SOURCE_BONUS: i32 = 450;
const CONTEXT_NORMALIZED_EXACT_BONUS: i32 = 420;
const CONTEXT_NORMALIZED_CONTAINS_BONUS: i32 = 180;
const CONTEXT_REVERSE_CONTAINS_BONUS: i32 = 80;
const CONTEXT_CALLABLE_SUFFIX_BONUS: i32 = 320;
const CONTEXT_PATH_KEYWORD_BONUS: i32 = 80;
const CONTEXT_BASENAME_CALLABLE_BONUS: i32 = 520;
const CONTEXT_NONCALLABLE_SHADOW_PENALTY: i32 = 260;
const CONTEXT_SYMBOL_TERM_BONUS: i32 = 35;
const CONTEXT_PATH_TERM_BONUS: i32 = 10;
const CONTEXT_MULTI_TERM_CALLABLE_BONUS: i32 = 520;
const CONTEXT_ACTION_NAME_BONUS: i32 = 260;
const CONTEXT_NO_CORE_SYMBOL_PENALTY: i32 = 1000;
const CONTEXT_WEAK_CORE_MATCH_PENALTY: i32 = 1200;
const CONTEXT_STRONG_CORE_MATCH_BONUS: i32 = 600;
const CONTEXT_PATH_CORE_MATCH_BONUS: i32 = 200;
const CONTEXT_POOR_CORE_PATH_PENALTY: i32 = 360;
const CONTEXT_SYMBOL_CORE_TERM_BONUS: i32 = 90;
const CONTEXT_PATH_CORE_TERM_BONUS: i32 = 25;
const CONTEXT_TEST_PATH_PENALTY: i32 = 60;
const CONTEXT_MULTI_TERM_SYMBOL_BONUS: i32 = 120;
const CONTEXT_MULTI_TERM_PATH_BONUS: i32 = 55;
const CONTEXT_MULTI_TERM_CALLABLE_KIND_BONUS: i32 = 120;
const CONTEXT_MULTI_TERM_ACTION_BONUS: i32 = 140;
const CONTEXT_MULTI_TERM_RUNTIME_BONUS: i32 = 80;
const CONTEXT_SNIPPET_LINE_MATCH_BONUS: i32 = 20;
const CONTEXT_SNIPPET_WORD_MATCH_BONUS: i32 = 30;
const CONTEXT_SNIPPET_PATH_MATCH_BONUS: i32 = 15;
const CONTEXT_SNIPPET_RELEVANT_SYMBOL_BONUS: i32 = 40;
const CONTEXT_SNIPPET_COMMENT_PENALTY: i32 = 20;
const CONTEXT_SNIPPET_SHORT_KEYWORD_PENALTY: i32 = 10;

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

struct ScoredSearchResult {
    score: i32,
    result: SearchResult,
}

struct ScoredContextSymbol {
    score: i32,
    result: SymbolResult,
}

struct ImportResolution {
    deps: Vec<String>,
    unresolved: Vec<String>,
}

pub struct DepGraph {
    forward: HashMap<String, Vec<String>>,
    reverse: HashMap<String, HashSet<String>>,
    unresolved: HashMap<String, Vec<UnresolvedImport>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            unresolved: HashMap::new(),
        }
    }

    pub fn set_resolution(
        &mut self,
        path: &str,
        deps: Vec<String>,
        unresolved: Vec<UnresolvedImport>,
    ) {
        if let Some(old_deps) = self.forward.get(path) {
            for dep in old_deps {
                if let Some(set) = self.reverse.get_mut(dep) {
                    set.remove(path);
                }
            }
        }

        for dep in &deps {
            self.reverse
                .entry(dep.clone())
                .or_default()
                .insert(path.to_string());
        }
        self.forward.insert(path.to_string(), deps);
        if unresolved.is_empty() {
            self.unresolved.remove(path);
        } else {
            self.unresolved.insert(path.to_string(), unresolved);
        }
    }

    pub fn clear(&mut self) {
        self.forward.clear();
        self.reverse.clear();
        self.unresolved.clear();
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(deps) = self.forward.remove(path) {
            for dep in deps {
                if let Some(set) = self.reverse.get_mut(&dep) {
                    set.remove(path);
                }
            }
        }
        self.reverse.remove(path);
        self.unresolved.remove(path);
    }

    pub fn get_imported_by(&self, path: &str) -> Vec<String> {
        self.reverse
            .get(path)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_depends_on(&self, path: &str) -> Vec<String> {
        self.forward.get(path).cloned().unwrap_or_default()
    }

    pub fn get_unresolved_imports(&self, path: &str) -> Vec<UnresolvedImport> {
        self.unresolved.get(path).cloned().unwrap_or_default()
    }

    pub fn unresolved_imports(&self) -> Vec<UnresolvedImport> {
        self.unresolved
            .values()
            .flat_map(|imports| imports.iter().cloned())
            .collect()
    }

    pub fn get_transitive(&self, path: &str, reverse: bool) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut stack = vec![path.to_string()];
        let mut result = Vec::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if current != path {
                result.push(current.clone());
            }

            let neighbors = if reverse {
                self.get_imported_by(&current)
            } else {
                self.get_depends_on(&current)
            };

            for neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }

        result
    }
}

impl Default for DepGraph {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Engine {
    outlines: HashMap<String, FileOutline>,
    file_meta: HashMap<String, FileMeta>,
    contents: HashMap<String, String>,
    content_cache: ContentCache,
    symbol_index: SymbolIndex,
    trigram_index: TrigramIndex,
    word_index: WordIndex,
    dep_graph: DepGraph,
    store: Store,
}

impl Engine {
    pub fn new(cache_capacity: u32) -> Self {
        Self {
            outlines: HashMap::new(),
            file_meta: HashMap::new(),
            contents: HashMap::new(),
            content_cache: ContentCache::new(cache_capacity),
            symbol_index: SymbolIndex::new(),
            trigram_index: TrigramIndex::new(),
            word_index: WordIndex::new(),
            dep_graph: DepGraph::new(),
            store: Store::new(),
        }
    }

    pub fn index_file(&mut self, path: &str, content: &str) {
        self.index_file_with_modified(path, content, now_ms());
    }

    pub fn index_file_with_modified(&mut self, path: &str, content: &str, modified_ms: u64) {
        self.index_file_with_op(path, content, modified_ms, Op::Snapshot, true);
    }

    pub fn index_edited_file(&mut self, path: &str, content: &str, op: Op) {
        self.index_file_with_op(path, content, now_ms(), op, true);
    }

    fn index_file_with_op(
        &mut self,
        path: &str,
        content: &str,
        modified_ms: u64,
        op: Op,
        rebuild_deps: bool,
    ) {
        let language = detect_language(path);
        let line_count = content.lines().count().max(1) as u32;
        let byte_size = content.len() as u64;

        let outline = parser::parse_file(path, language, content).unwrap_or_else(|| {
            let mut o = FileOutline::new(path.to_string(), language);
            o.line_count = line_count;
            o.byte_size = byte_size;
            o
        });

        self.symbol_index.index_file(&outline);
        self.trigram_index.index_file(path, content);
        self.word_index.index_file(path, content);
        self.content_cache
            .put(path.to_string(), content.to_string());
        self.contents.insert(path.to_string(), content.to_string());
        self.file_meta.insert(
            path.to_string(),
            FileMeta {
                language,
                line_count,
                byte_size,
                symbol_count: outline.symbol_count() as u32,
                modified_ms,
                indexed: true,
            },
        );
        self.outlines.insert(path.to_string(), outline);
        if rebuild_deps {
            self.rebuild_dep_graph();
        }
        match op {
            Op::Snapshot => {
                self.store
                    .record_snapshot(path, byte_size, hash_content(content));
            }
            Op::Replace | Op::Insert | Op::Delete | Op::Create => {
                self.store
                    .record_edit(path, 0, op, hash_content(content), byte_size);
            }
            Op::Tombstone => {
                self.store.record_delete(path, 0);
            }
        }
    }

    pub fn index_file_meta_only(&mut self, path: &str, byte_size: u64, modified_ms: u64) {
        self.index_file_meta_only_no_rebuild(path, byte_size, modified_ms);
        self.rebuild_dep_graph();
    }

    pub fn remove_file(&mut self, path: &str) {
        self.outlines.remove(path);
        self.file_meta.remove(path);
        self.contents.remove(path);
        self.content_cache.remove(path);
        self.symbol_index.remove_file(path);
        self.trigram_index.remove_file(path);
        self.word_index.remove_file(path);
        self.dep_graph.remove(path);
        self.rebuild_dep_graph();
        self.store.record_delete(path, 0);
    }

    pub fn get_outline(&self, path: &str) -> Option<&FileOutline> {
        self.outlines.get(path)
    }

    pub fn find_symbol(&self, name: &str) -> Vec<SymbolResult> {
        self.symbol_index
            .find_all(name)
            .into_iter()
            .map(|loc| {
                let symbol = self
                    .outlines
                    .get(&loc.path)
                    .and_then(|o| {
                        o.symbols
                            .iter()
                            .find(|s| s.name == name && s.kind == loc.kind)
                    })
                    .cloned()
                    .unwrap_or_else(|| Symbol {
                        name: name.to_string(),
                        kind: loc.kind,
                        line_start: loc.line_start,
                        line_end: loc.line_end,
                        detail: None,
                    });
                SymbolResult {
                    path: loc.path,
                    symbol,
                }
            })
            .collect()
    }

    pub fn fuzzy_symbols(&self, query: &str, max_results: usize) -> Vec<SymbolSearchResult> {
        let query_norm = context_normalize(query);
        if query_norm.is_empty() {
            return Vec::new();
        }

        let mut scored = Vec::new();
        for (path, outline) in &self.outlines {
            for symbol in &outline.symbols {
                if symbol.kind == SymbolKind::Import {
                    continue;
                }
                let symbol_norm = context_normalize(&symbol.name);
                let path_norm = context_normalize(path);
                let mut score = symbol_kind_context_score(symbol.kind);

                if symbol_norm == query_norm {
                    score += 1000;
                } else if symbol_norm.contains(&query_norm) {
                    score += 700;
                } else if query_norm.contains(&symbol_norm) && symbol_norm.len() >= 4 {
                    score += 320;
                } else if fuzzy_match_score(&query_norm, &symbol_norm).is_some() {
                    score += 220;
                } else {
                    continue;
                }

                if path_norm.contains(&query_norm) {
                    score += 80;
                }
                if is_test_like_path(path) {
                    score -= 60;
                }

                scored.push((score, path.clone(), symbol.clone()));
            }
        }

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.line_start.cmp(&b.2.line_start))
                .then_with(|| a.2.name.cmp(&b.2.name))
        });

        let max_score = scored.first().map(|entry| entry.0.max(1)).unwrap_or(1) as f32;
        scored
            .into_iter()
            .take(max_results)
            .map(|(raw_score, path, symbol)| SymbolSearchResult {
                path,
                name: symbol.name,
                kind: symbol.kind.to_string(),
                line_start: symbol.line_start,
                line_end: symbol.line_end,
                detail: symbol.detail,
                score: raw_score as f32 / max_score,
                raw_score,
            })
            .collect()
    }

    pub fn search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let words: Vec<&str> = query_lower.split_whitespace().collect();

        if words.len() > 1 {
            return self.search_multi_word(&words, max_results);
        }

        let single_word = words.first().copied().unwrap_or(&query_lower);
        let mut results = Vec::new();
        let mut seen: HashSet<(String, u32)> = HashSet::new();

        let word_hits = self.word_index.search(single_word);
        for (path, line_num) in word_hits {
            if results.len() >= max_results {
                return results;
            }
            if seen.insert((path.clone(), line_num)) {
                if let Some(line_text) = self.get_line(&path, line_num) {
                    results.push(SearchResult {
                        path,
                        line_num,
                        line_text,
                    });
                }
            }
        }

        if results.len() < max_results {
            let prefix_hits = self.word_index.search_prefix(single_word);
            for (path, line_num, _word) in prefix_hits {
                if results.len() >= max_results {
                    return results;
                }
                if seen.insert((path.clone(), line_num)) {
                    if let Some(line_text) = self.get_line(&path, line_num) {
                        results.push(SearchResult {
                            path,
                            line_num,
                            line_text,
                        });
                    }
                }
            }
        }

        if results.len() < max_results {
            let candidates = self.trigram_index.candidates(single_word);
            for path in candidates {
                if results.len() >= max_results {
                    return results;
                }
                if let Some(content) = self.content_for(&path) {
                    for (line_idx, line) in content.lines().enumerate() {
                        if results.len() >= max_results {
                            return results;
                        }
                        let line_num = (line_idx + 1) as u32;
                        if line.to_lowercase().contains(single_word)
                            && seen.insert((path.clone(), line_num))
                        {
                            results.push(SearchResult {
                                path: path.clone(),
                                line_num,
                                line_text: line.to_string(),
                            });
                        }
                    }
                }
            }
        }

        if results.is_empty() && self.outlines.len() < 100 {
            for (path, _) in &self.outlines {
                if results.len() >= max_results {
                    return results;
                }
                if let Some(content) = self.content_for(path) {
                    for (line_idx, line) in content.lines().enumerate() {
                        if results.len() >= max_results {
                            return results;
                        }
                        let line_num = (line_idx + 1) as u32;
                        if line.to_lowercase().contains(single_word)
                            && seen.insert((path.clone(), line_num))
                        {
                            results.push(SearchResult {
                                path: path.clone(),
                                line_num,
                                line_text: line.to_string(),
                            });
                        }
                    }
                }
            }
        }

        results
    }

    fn search_multi_word(&self, words: &[&str], max_results: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut seen: HashSet<(String, u32)> = HashSet::new();

        let first_word = words[0];
        let mut candidate_paths: Vec<String> = Vec::new();

        let word_hits = self.word_index.search(first_word);
        for (path, _line_num) in &word_hits {
            if !candidate_paths.contains(path) {
                candidate_paths.push(path.clone());
            }
        }

        let trigram_candidates = self.trigram_index.candidates(first_word);
        for path in trigram_candidates {
            if !candidate_paths.contains(&path) {
                candidate_paths.push(path);
            }
        }

        if candidate_paths.is_empty() {
            candidate_paths = self.file_meta.keys().cloned().collect();
        }

        for path in candidate_paths {
            if results.len() >= max_results {
                return results;
            }
            if let Some(content) = self.content_for(&path) {
                for (line_idx, line) in content.lines().enumerate() {
                    if results.len() >= max_results {
                        return results;
                    }
                    let line_lower = line.to_lowercase();
                    let contains_all = words.iter().all(|w| line_lower.contains(w));
                    if contains_all {
                        let line_num = (line_idx + 1) as u32;
                        if seen.insert((path.clone(), line_num)) {
                            results.push(SearchResult {
                                path: path.clone(),
                                line_num,
                                line_text: line.to_string(),
                            });
                        }
                    }
                }
            }
        }

        results
    }

    pub fn search_regex(
        &self,
        pattern: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, String> {
        let re = Regex::new(pattern).map_err(|e| format!("Invalid regex: {}", e))?;
        let mut results = Vec::new();
        let mut seen: HashSet<(String, u32)> = HashSet::new();

        for (path, _) in &self.outlines {
            if results.len() >= max_results {
                break;
            }
            if let Some(content) = self.content_for(path) {
                for (line_idx, line) in content.lines().enumerate() {
                    if results.len() >= max_results {
                        break;
                    }
                    if re.is_match(line) {
                        let line_num = (line_idx + 1) as u32;
                        if seen.insert((path.clone(), line_num)) {
                            results.push(SearchResult {
                                path: path.clone(),
                                line_num,
                                line_text: line.to_string(),
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    pub fn search_rich(
        &self,
        query: &str,
        options: &SearchOptions,
    ) -> Result<Vec<RichSearchResult>, String> {
        let max_results = options.max_results.max(1);
        let raw = if options.regex {
            self.search_regex(query, max_results.saturating_mul(4))?
        } else {
            self.search(query, max_results.saturating_mul(4))
        };

        let mut results = Vec::new();
        for result in raw {
            if let Some(pattern) = &options.path_glob {
                if !matches_path_glob(pattern, &result.path) {
                    continue;
                }
            }

            let language = self
                .file_meta
                .get(&result.path)
                .map(|meta| meta.language)
                .unwrap_or_else(|| detect_language(&result.path));
            if options.compact && is_comment_or_blank(&result.line_text, language) {
                continue;
            }

            let scope = if options.scope {
                self.enclosing_symbol(&result.path, result.line_num)
                    .cloned()
            } else {
                None
            };

            results.push(RichSearchResult {
                path: result.path,
                line_num: result.line_num,
                line_text: result.line_text,
                scope,
            });

            if results.len() >= max_results {
                break;
            }
        }

        Ok(results)
    }

    pub fn search_word(&self, word: &str) -> Vec<SearchResult> {
        self.word_index
            .search(word)
            .into_iter()
            .filter_map(|(path, line_num)| {
                self.get_line(&path, line_num)
                    .map(|line_text| SearchResult {
                        path,
                        line_num,
                        line_text,
                    })
            })
            .collect()
    }

    pub fn file_map(&self) -> Vec<(String, FileMeta)> {
        let mut entries: Vec<(&String, &FileMeta)> = self.file_meta.iter().collect();
        entries.sort_by_key(|(path, _)| path.as_str());
        entries
            .into_iter()
            .map(|(path, meta)| (path.clone(), meta.clone()))
            .collect()
    }

    pub fn filtered_files(
        &self,
        options: &FileFilterOptions,
    ) -> (Vec<(String, FileMeta)>, usize, bool) {
        let language = options.language.as_ref().map(|value| value.to_lowercase());
        let path_prefix = options
            .path_prefix
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(normalize_filter_prefix);

        let mut entries =
            self.file_map()
                .into_iter()
                .filter(|(path, meta)| {
                    path_prefix.as_ref().is_none_or(|prefix| {
                        path == prefix || path.starts_with(&format!("{prefix}/"))
                    }) && options
                        .path_glob
                        .as_deref()
                        .is_none_or(|glob| match_glob(glob, path))
                        && language
                            .as_deref()
                            .is_none_or(|language| meta.language.as_str() == language)
                        && options
                            .min_lines
                            .is_none_or(|min_lines| meta.line_count >= min_lines)
                        && options
                            .max_lines
                            .is_none_or(|max_lines| meta.line_count <= max_lines)
                })
                .collect::<Vec<_>>();
        let total = entries.len();
        if let Some(max_results) = options.max_results {
            entries.truncate(max_results);
        }
        let truncated = entries.len() < total;
        (entries, total, truncated)
    }

    pub fn get_imported_by(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_imported_by(path)
    }

    pub fn get_depends_on(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_depends_on(path)
    }

    pub fn get_unresolved_imports(&self, path: &str) -> Vec<UnresolvedImport> {
        self.dep_graph.get_unresolved_imports(path)
    }

    pub fn unresolved_imports(&self) -> Vec<UnresolvedImport> {
        let mut imports = self.dep_graph.unresolved_imports();
        imports.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.line_start.cmp(&b.line_start))
                .then_with(|| a.import.cmp(&b.import))
        });
        imports
    }

    pub fn get_transitive_imported_by(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_transitive(path, true)
    }

    pub fn get_transitive_depends_on(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_transitive(path, false)
    }

    pub fn get_hot_files(&self, limit: usize) -> Vec<(String, FileMeta)> {
        let mut entries: Vec<(&String, &FileMeta)> = self.file_meta.iter().collect();
        entries.sort_by(|a, b| {
            b.1.modified_ms
                .cmp(&a.1.modified_ms)
                .then_with(|| b.1.byte_size.cmp(&a.1.byte_size))
                .then_with(|| a.0.cmp(b.0))
        });
        entries
            .into_iter()
            .take(limit)
            .map(|(p, m)| (p.clone(), m.clone()))
            .collect()
    }

    pub fn find_callers(&self, symbol_name: &str, max_results: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut seen: HashSet<(String, u32)> = HashSet::new();

        let definitions = self.symbol_index.find_all(symbol_name);
        let def_locations: HashSet<(String, u32)> = definitions
            .iter()
            .map(|loc| (loc.path.clone(), loc.line_start))
            .collect();

        let occurrences = self.word_index.search(symbol_name);

        for (path, line_num) in occurrences {
            if results.len() >= max_results {
                break;
            }

            if def_locations.contains(&(path.clone(), line_num)) {
                continue;
            }

            if seen.insert((path.clone(), line_num)) {
                if let Some(line_text) = self.get_line(&path, line_num) {
                    results.push(SearchResult {
                        path,
                        line_num,
                        line_text,
                    });
                }
            }
        }

        results
    }

    pub fn build_context_with_options(&self, task: &str, options: &ContextOptions) -> String {
        let details = self.build_context_details_with_options(task, options);
        render_context_details(&details)
    }
}

fn render_context_details(details: &ContextDetails) -> String {
    let mut output = String::new();
    output.push_str(&format!("## Context for: {}\n\n", details.task));
    if let Some(note) = &details.note {
        output.push_str(&format!("{}\n\n", note));
    }
    if !details.suggested_next_steps.is_empty() {
        output.push_str("### Suggested Next Steps\n\n");
        for step in &details.suggested_next_steps {
            output.push_str(&format!("- {step}\n"));
        }
        output.push('\n');
    }

    if !details.relevant_symbols.is_empty() {
        output.push_str("### Relevant Symbols\n\n");
        for sym in &details.relevant_symbols {
            output.push_str(&format!(
                "- {} ({}): {}:{}-{}\n",
                sym.name, sym.kind, sym.path, sym.line_start, sym.line_end
            ));
        }
        output.push('\n');

        output.push_str("### Relevant Symbol Bodies\n\n");
        for sym in &details.relevant_symbols {
            output.push_str(&format!(
                "#### {}:{}-{} {}\n\n",
                sym.path, sym.content_line_start, sym.content_line_end, sym.name
            ));
            output.push_str("```text\n");
            output.push_str(&sym.content);
            if !sym.content.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n\n");
        }
    }

    if !details.snippets.is_empty() {
        output.push_str("### Relevant Code Snippets\n\n");
        for result in &details.snippets {
            output.push_str(&format!(
                "{}:{}: {}\n",
                result.path, result.line_num, result.line_text
            ));
        }
    }

    output
}

impl Engine {
    pub fn build_context_details(&self, task: &str, max_results: usize) -> ContextDetails {
        self.build_context_details_with_options(
            task,
            &ContextOptions {
                max_results,
                ..ContextOptions::default()
            },
        )
    }

    pub fn build_context_details_with_options(
        &self,
        task: &str,
        options: &ContextOptions,
    ) -> ContextDetails {
        let keywords = context_keywords(task);
        let max_results = options.max_results.max(1);
        let relevant_symbols = self.ranked_context_symbols(&keywords, options, 5);

        let mut snippets =
            self.ranked_context_snippets(&keywords, &relevant_symbols, options, max_results);
        let explicit_query = context_query_is_explicit(task);

        ContextDetails {
            task: task.to_string(),
            keywords,
            max_results,
            confidence: if explicit_query {
                "medium".to_string()
            } else {
                "low".to_string()
            },
            note: (!explicit_query).then(|| {
                "Low-confidence brief: this tool bundles context from explicit symbols, path fragments, and scoped keywords; it is not natural-language QA.".to_string()
            }),
            suggested_next_steps: if explicit_query {
                Vec::new()
            } else {
                vec![
                    "Add --path-prefix or --path-glob to scope the search.".to_string(),
                    "Run symbol-search for likely symbol names.".to_string(),
                    "Run text-search for concrete terms from the task.".to_string(),
                ]
            },
            relevant_symbols,
            snippets: std::mem::take(&mut snippets),
        }
    }

    fn ranked_context_symbols(
        &self,
        keywords: &[String],
        options: &ContextOptions,
        max_symbols: usize,
    ) -> Vec<ContextSymbol> {
        let mut scored: Vec<ScoredContextSymbol> = Vec::new();
        let mut seen: HashSet<(String, String, SymbolKind, u32)> = HashSet::new();

        for keyword in keywords {
            for result in self.find_symbol(keyword) {
                if !self.context_path_allowed(&result.path, options) {
                    continue;
                }
                push_context_symbol_candidate(
                    &mut scored,
                    &mut seen,
                    self.context_symbol_score(keyword, &result, keywords)
                        + CONTEXT_INDEXED_SYMBOL_SOURCE_BONUS,
                    result,
                );
            }

            for (path, outline) in &self.outlines {
                if !self.context_path_allowed(path, options) {
                    continue;
                }
                for symbol in &outline.symbols {
                    if !symbol.name.eq_ignore_ascii_case(keyword) {
                        continue;
                    }
                    push_context_symbol_candidate(
                        &mut scored,
                        &mut seen,
                        self.context_symbol_score(
                            keyword,
                            &SymbolResult {
                                path: path.clone(),
                                symbol: symbol.clone(),
                            },
                            keywords,
                        ) + CONTEXT_OUTLINE_SYMBOL_SOURCE_BONUS,
                        SymbolResult {
                            path: path.clone(),
                            symbol: symbol.clone(),
                        },
                    );
                }
            }

            if keyword.len() < 3 {
                continue;
            }

            for (path, path_score) in self.fuzzy_find(keyword, 5) {
                if !self.context_path_allowed(&path, options) {
                    continue;
                }
                let Some(outline) = self.outlines.get(&path) else {
                    continue;
                };
                for symbol in &outline.symbols {
                    if symbol.kind == SymbolKind::Import {
                        continue;
                    }
                    let result = SymbolResult {
                        path: path.clone(),
                        symbol: symbol.clone(),
                    };
                    let score = self.context_symbol_score(keyword, &result, keywords)
                        + path_score.round() as i32;
                    if score < 80 {
                        continue;
                    }
                    push_context_symbol_candidate(&mut scored, &mut seen, score, result);
                }
            }
        }

        let core_terms = context_core_terms(keywords);
        if !core_terms.is_empty() {
            for (path, outline) in &self.outlines {
                if !self.context_path_allowed(path, options) {
                    continue;
                }
                for symbol in &outline.symbols {
                    if symbol.kind == SymbolKind::Import {
                        continue;
                    }
                    let result = SymbolResult {
                        path: path.clone(),
                        symbol: symbol.clone(),
                    };
                    let score = self.context_multi_term_symbol_score(&result, &core_terms);
                    if score >= 260 {
                        push_context_symbol_candidate(&mut scored, &mut seen, score, result);
                    }
                }
            }
        }

        scored.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.result.path.cmp(&b.result.path))
                .then_with(|| a.result.symbol.line_start.cmp(&b.result.symbol.line_start))
                .then_with(|| a.result.symbol.name.cmp(&b.result.symbol.name))
        });

        scored
            .into_iter()
            .filter_map(|entry| {
                self.context_symbol_from_result(entry.result, MAX_CONTEXT_SYMBOL_LINES)
            })
            .take(max_symbols)
            .collect()
    }

    fn context_symbol_from_result(
        &self,
        result: SymbolResult,
        max_lines: u32,
    ) -> Option<ContextSymbol> {
        let (content_line_start, content_line_end, content) =
            self.symbol_source_bounded(&result.path, &result.symbol, max_lines)?;
        Some(ContextSymbol {
            path: result.path,
            name: result.symbol.name,
            kind: result.symbol.kind.to_string(),
            line_start: result.symbol.line_start,
            line_end: result.symbol.line_end,
            detail: result.symbol.detail,
            content_line_start,
            content_line_end,
            content,
        })
    }

    fn context_symbol_score(
        &self,
        keyword: &str,
        result: &SymbolResult,
        keywords: &[String],
    ) -> i32 {
        let symbol_name = &result.symbol.name;
        let symbol_norm = context_normalize(symbol_name);
        let keyword_norm = context_normalize(keyword);
        let path_norm = context_normalize(&result.path);
        let basename_norm = result
            .path
            .rsplit('/')
            .next()
            .map(|name| name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name))
            .map(context_normalize)
            .unwrap_or_default();
        let callable = matches!(
            result.symbol.kind,
            SymbolKind::Function | SymbolKind::Method
        );
        let mut score = symbol_kind_context_score(result.symbol.kind);

        if symbol_name == keyword {
            score += CONTEXT_EXACT_SYMBOL_BONUS;
        } else if has_identifier_case_signal(keyword) && symbol_name.eq_ignore_ascii_case(keyword) {
            score += CONTEXT_CASE_INSENSITIVE_SYMBOL_BONUS;
        }

        if !keyword_norm.is_empty() {
            if symbol_norm == keyword_norm {
                score += CONTEXT_NORMALIZED_EXACT_BONUS;
            } else if symbol_norm.contains(&keyword_norm) {
                score += CONTEXT_NORMALIZED_CONTAINS_BONUS;
            } else if keyword_norm.len() >= 5 && keyword_norm.contains(&symbol_norm) {
                score += CONTEXT_REVERSE_CONTAINS_BONUS;
            }

            if callable
                && symbol_norm.ends_with(&keyword_norm)
                && symbol_norm.len() > keyword_norm.len()
            {
                score += CONTEXT_CALLABLE_SUFFIX_BONUS;
            }

            if path_norm.contains(&keyword_norm) {
                score += CONTEXT_PATH_KEYWORD_BONUS;
            }
        }

        if callable && !basename_norm.is_empty() && symbol_norm == basename_norm {
            score += CONTEXT_BASENAME_CALLABLE_BONUS;
        }

        if !callable
            && !symbol_norm.is_empty()
            && self.outlines.get(&result.path).is_some_and(|outline| {
                outline.symbols.iter().any(|symbol| {
                    matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method)
                        && context_normalize(&symbol.name).ends_with(&symbol_norm)
                        && context_normalize(&symbol.name).len() > symbol_norm.len()
                })
            })
        {
            score -= CONTEXT_NONCALLABLE_SHADOW_PENALTY;
        }

        let mut matched_context_terms = 0;
        for term in context_terms(keywords) {
            if term.len() < 3 {
                continue;
            }
            if symbol_norm.contains(&term) {
                matched_context_terms += 1;
                score += CONTEXT_SYMBOL_TERM_BONUS;
            }
            if path_norm.contains(&term) {
                score += CONTEXT_PATH_TERM_BONUS;
            }
        }

        if callable && matched_context_terms >= 2 {
            score += CONTEXT_MULTI_TERM_CALLABLE_BONUS;
            if symbol_norm.starts_with("create") || symbol_norm.starts_with("use") {
                score += CONTEXT_ACTION_NAME_BONUS;
            }
        }

        let core_terms = context_core_terms(keywords);
        if !core_terms.is_empty() {
            let symbol_core_matches = core_terms
                .iter()
                .filter(|term| symbol_norm.contains(term.as_str()))
                .count();
            let path_core_matches = core_terms
                .iter()
                .filter(|term| path_norm.contains(term.as_str()))
                .count();
            if symbol_core_matches == 0 {
                score -= CONTEXT_NO_CORE_SYMBOL_PENALTY;
            } else if symbol_core_matches == 1 && path_core_matches == 0 && core_terms.len() >= 3 {
                score -= CONTEXT_WEAK_CORE_MATCH_PENALTY;
            } else if symbol_core_matches >= 2 {
                score += CONTEXT_STRONG_CORE_MATCH_BONUS;
            } else if path_core_matches >= 1 {
                score += CONTEXT_PATH_CORE_MATCH_BONUS;
            }
            if symbol_core_matches == 0 && path_core_matches < 2 {
                score -= CONTEXT_POOR_CORE_PATH_PENALTY;
            } else {
                score += (symbol_core_matches as i32 * CONTEXT_SYMBOL_CORE_TERM_BONUS)
                    + (path_core_matches as i32 * CONTEXT_PATH_CORE_TERM_BONUS);
            }
        }

        if is_test_like_path(&result.path) {
            score -= CONTEXT_TEST_PATH_PENALTY;
        }

        score
    }

    fn context_multi_term_symbol_score(&self, result: &SymbolResult, terms: &[String]) -> i32 {
        let symbol_norm = context_normalize(&result.symbol.name);
        let path_norm = context_normalize(&result.path);
        let mut matched_symbol_terms = 0;
        let mut matched_path_terms = 0;
        let mut score = symbol_kind_context_score(result.symbol.kind);

        for term in terms {
            if symbol_norm.contains(term) {
                matched_symbol_terms += 1;
                score += CONTEXT_MULTI_TERM_SYMBOL_BONUS;
            }
            if path_norm.contains(term) {
                matched_path_terms += 1;
                score += CONTEXT_MULTI_TERM_PATH_BONUS;
            }
        }

        if matched_symbol_terms == 0 && matched_path_terms < 2 {
            return 0;
        }
        if matched_symbol_terms + matched_path_terms < 2 {
            return 0;
        }
        if matches!(
            result.symbol.kind,
            SymbolKind::Function | SymbolKind::Method
        ) {
            score += CONTEXT_MULTI_TERM_CALLABLE_KIND_BONUS;
        }
        if symbol_norm.starts_with("create") || symbol_norm.starts_with("run") {
            score += CONTEXT_MULTI_TERM_ACTION_BONUS;
        }
        if symbol_norm.contains("runtime") {
            score += CONTEXT_MULTI_TERM_RUNTIME_BONUS;
        }
        if is_test_like_path(&result.path) {
            score -= CONTEXT_TEST_PATH_PENALTY;
        }
        score
    }

    fn ranked_context_snippets(
        &self,
        keywords: &[String],
        relevant_symbols: &[ContextSymbol],
        options: &ContextOptions,
        max_results: usize,
    ) -> Vec<SearchResult> {
        let mut scored: Vec<ScoredSearchResult> = Vec::new();
        for keyword in keywords {
            let results = self.search(keyword, 8);
            for result in results {
                if !self.context_path_allowed(&result.path, options) {
                    continue;
                }
                if scored
                    .iter()
                    .any(|x| x.result.path == result.path && x.result.line_num == result.line_num)
                {
                    continue;
                }
                let score = self.context_snippet_score(keyword, &result, relevant_symbols);
                scored.push(ScoredSearchResult { score, result });
            }
        }
        scored.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.result.path.cmp(&b.result.path))
                .then_with(|| a.result.line_num.cmp(&b.result.line_num))
        });
        scored
            .into_iter()
            .take(max_results)
            .map(|entry| entry.result)
            .collect()
    }

    fn context_path_allowed(&self, path: &str, options: &ContextOptions) -> bool {
        if let Some(prefix) = options
            .path_prefix
            .as_deref()
            .filter(|prefix| !prefix.is_empty())
            .map(normalize_filter_prefix)
        {
            if path != prefix && !path.starts_with(&format!("{prefix}/")) {
                return false;
            }
        }
        if let Some(glob) = options.path_glob.as_deref() {
            if !match_glob(glob, path) {
                return false;
            }
        }
        if let Some(language) = options.language.as_deref() {
            let language = language.to_lowercase();
            if self
                .file_meta
                .get(path)
                .is_none_or(|meta| meta.language.as_str() != language)
            {
                return false;
            }
        }
        true
    }

    fn context_snippet_score(
        &self,
        keyword: &str,
        result: &SearchResult,
        relevant_symbols: &[ContextSymbol],
    ) -> i32 {
        let keyword_lower = keyword.to_lowercase();
        let path_lower = result.path.to_lowercase();
        let line_lower = result.line_text.to_lowercase();
        let mut score = 0;

        if line_lower.contains(&keyword_lower) {
            score += CONTEXT_SNIPPET_LINE_MATCH_BONUS;
        }
        if result
            .line_text
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .any(|word| word.eq_ignore_ascii_case(keyword))
        {
            score += CONTEXT_SNIPPET_WORD_MATCH_BONUS;
        }
        if path_lower.contains(&keyword_lower) {
            score += CONTEXT_SNIPPET_PATH_MATCH_BONUS;
        }
        if relevant_symbols
            .iter()
            .any(|symbol| symbol.path == result.path || symbol.name.eq_ignore_ascii_case(keyword))
        {
            score += CONTEXT_SNIPPET_RELEVANT_SYMBOL_BONUS;
        }

        let language = self
            .file_meta
            .get(&result.path)
            .map(|meta| meta.language)
            .unwrap_or_else(|| detect_language(&result.path));
        if is_comment_or_blank(&result.line_text, language) {
            score -= CONTEXT_SNIPPET_COMMENT_PENALTY;
        }
        if keyword.len() <= 3 {
            score -= CONTEXT_SNIPPET_SHORT_KEYWORD_PENALTY;
        }
        score
    }

    pub fn get_changes(&self, since_seq: u64) -> Vec<(String, u64, String)> {
        let changes = self.store.changes_since_detailed(since_seq);
        changes
            .into_iter()
            .map(|c| {
                let op_str = format!("{:?}", c.op);
                (c.path, c.seq, op_str)
            })
            .collect()
    }

    pub fn read_file(
        &self,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
    ) -> Option<String> {
        let stub;
        let content = if let Some(content) = self.content_for(path) {
            content
        } else {
            let meta = self.file_meta.get(path)?;
            if meta.indexed {
                return None;
            }
            stub = unindexed_file_stub(path, meta);
            &stub
        };
        let lines: Vec<&str> = content.lines().collect();

        match (line_start, line_end) {
            (Some(start), Some(end)) => {
                let start_idx = (start.saturating_sub(1)) as usize;
                let end_idx = (end as usize).min(lines.len());
                if start_idx >= lines.len() || start_idx >= end_idx {
                    return Some(String::new());
                }
                Some(lines[start_idx..end_idx].join("\n"))
            }
            (Some(start), None) => {
                let start_idx = (start.saturating_sub(1)) as usize;
                if start_idx >= lines.len() {
                    return Some(String::new());
                }
                Some(lines[start_idx..].join("\n"))
            }
            (None, Some(end)) => {
                let end_idx = (end as usize).min(lines.len());
                Some(lines[..end_idx].join("\n"))
            }
            (None, None) => Some(content.to_string()),
        }
    }

    pub fn read_file_rich(
        &self,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
        compact: bool,
        if_hash: Option<&str>,
    ) -> Option<ReadFileResult> {
        let stub;
        let content = if let Some(content) = self.content_for(path) {
            content
        } else {
            let meta = self.file_meta.get(path)?;
            if meta.indexed {
                return None;
            }
            stub = unindexed_file_stub(path, meta);
            &stub
        };
        let hash = hash_content(content);
        let hash_hex = format!("{hash:x}");
        if if_hash.is_some_and(|expected| expected.eq_ignore_ascii_case(&hash_hex)) {
            return Some(ReadFileResult {
                content: String::new(),
                hash,
                unchanged: true,
            });
        }

        let mut selected = self.read_file(path, line_start, line_end)?;
        if compact {
            let language = self
                .file_meta
                .get(path)
                .map(|meta| meta.language)
                .unwrap_or_else(|| detect_language(path));
            selected = selected
                .lines()
                .filter(|line| !is_comment_or_blank(line, language))
                .collect::<Vec<_>>()
                .join("\n");
        }

        Some(ReadFileResult {
            content: selected,
            hash,
            unchanged: false,
        })
    }

    pub fn glob_files(&self, pattern: &str) -> Vec<String> {
        let mut results: Vec<String> = self
            .file_meta
            .keys()
            .filter(|path| match_glob(pattern, path))
            .cloned()
            .collect();
        results.sort();
        results
    }

    pub fn fuzzy_find(&self, pattern: &str, max_results: usize) -> Vec<(String, f32)> {
        let pattern_lower = pattern.to_lowercase();
        let pattern_chars: Vec<char> = pattern_lower.chars().collect();
        let mut results: Vec<(String, f32)> = Vec::new();

        for path in self.file_meta.keys() {
            let path_lower = path.to_lowercase();
            let filename = path_lower.rsplit('/').next().unwrap_or(&path_lower);

            let (score, matched) = if let Some(s) = fuzzy_match(&pattern_chars, filename) {
                (s + 10.0, true)
            } else if let Some(s) = fuzzy_match(&pattern_chars, &path_lower) {
                (s, true)
            } else {
                (0.0, false)
            };

            if matched && score > 0.0 {
                results.push((path.clone(), score));
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(max_results);
        results
    }

    pub fn list_dir(&self, dir: &str) -> Vec<(String, Option<&FileMeta>)> {
        let prefix = if dir.is_empty() || dir == "." {
            String::new()
        } else {
            format!("{}/", dir.trim_end_matches('/'))
        };

        let mut dirs: HashSet<String> = HashSet::new();
        let mut files: Vec<(String, Option<&FileMeta>)> = Vec::new();

        for (path, meta) in &self.file_meta {
            if let Some(rest) = path.strip_prefix(&prefix) {
                if let Some(slash_pos) = rest.find('/') {
                    let dir_name = &rest[..slash_pos];
                    dirs.insert(dir_name.to_string());
                } else {
                    files.push((rest.to_string(), Some(meta)));
                }
            }
        }

        let mut result: Vec<(String, Option<&FileMeta>)> = Vec::new();
        let mut sorted_dirs: Vec<String> = dirs.into_iter().collect();
        sorted_dirs.sort();
        for d in sorted_dirs {
            result.push((format!("{}/", d), None));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        result.extend(files);

        result
    }

    pub fn file_count(&self) -> usize {
        self.file_meta.len()
    }

    pub fn symbol_index_count(&self) -> usize {
        self.symbol_index.symbol_count()
    }

    pub fn word_index_count(&self) -> usize {
        self.word_index.unique_word_count()
    }

    pub fn word_index_file_count(&self) -> usize {
        self.word_index.file_count()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn content(&self, path: &str) -> Option<&str> {
        self.content_for(path)
    }

    pub fn enclosing_symbol(&self, path: &str, line_num: u32) -> Option<&Symbol> {
        self.outlines.get(path).and_then(|outline| {
            outline
                .symbols
                .iter()
                .filter(|sym| {
                    sym.kind != SymbolKind::Import
                        && sym.line_start <= line_num
                        && sym.line_end >= line_num
                })
                .max_by_key(|sym| sym.line_start)
        })
    }

    fn symbol_source_bounded(
        &self,
        path: &str,
        symbol: &Symbol,
        max_lines: u32,
    ) -> Option<(u32, u32, String)> {
        let outline = self.outlines.get(path)?;
        let start = symbol.line_start.max(1);
        let natural_end = symbol.line_end.min(outline.line_count);
        let capped_end = start
            .saturating_add(max_lines.saturating_sub(1))
            .min(natural_end)
            .max(start);
        self.read_file(path, Some(start), Some(capped_end))
            .map(|content| (start, capped_end, content))
    }

    fn get_line(&self, path: &str, line_num: u32) -> Option<String> {
        let content = self.content_for(path)?;
        content
            .lines()
            .nth((line_num - 1) as usize)
            .map(|s| s.to_string())
    }

    fn content_for(&self, path: &str) -> Option<&str> {
        self.content_cache
            .get(path)
            .or_else(|| self.contents.get(path).map(String::as_str))
    }

    fn resolve_imports(
        &self,
        path: &str,
        imports: &[String],
        language: Language,
    ) -> ImportResolution {
        let mut deps = Vec::new();
        let mut unresolved = Vec::new();
        for import in imports {
            if language == Language::Rust {
                deps.extend(self.resolve_rust_import(path, import));
            } else {
                let terms = import_terms(import);
                if let Some(candidate) = self.resolve_generic_import_terms(path, &terms) {
                    deps.push(candidate);
                } else if is_local_generic_import(&terms) {
                    unresolved.push(import.clone());
                }
            }
        }
        deps.sort();
        deps.dedup();
        unresolved.sort();
        unresolved.dedup();
        ImportResolution { deps, unresolved }
    }

    fn resolve_generic_import(&self, importer_path: &str, import: &str) -> Option<String> {
        let terms = import_terms(import);
        self.resolve_generic_import_terms(importer_path, &terms)
    }

    fn resolve_generic_import_terms(
        &self,
        importer_path: &str,
        terms: &[String],
    ) -> Option<String> {
        if terms.is_empty() {
            return None;
        }

        if let Some(candidate) = self.exact_import_match(importer_path, terms) {
            return Some(candidate);
        }

        if is_local_generic_import(terms) {
            return None;
        }

        let mut best_match: Option<(i32, &str)> = None;
        for candidate in self.file_meta.keys() {
            if candidate == importer_path {
                continue;
            }

            for term in terms {
                let Some(score) = import_match_score(term, candidate) else {
                    continue;
                };
                let should_replace = best_match.is_none_or(|(best_score, best_path)| {
                    score > best_score || (score == best_score && candidate.as_str() < best_path)
                });
                if should_replace {
                    best_match = Some((score, candidate));
                }
            }
        }

        best_match.map(|(_, candidate)| candidate.to_string())
    }

    fn resolve_rust_import(&self, importer_path: &str, import: &str) -> Vec<String> {
        let mut deps = Vec::new();
        let mut seen = HashSet::new();

        let module_groups = rust_import_module_path_groups(importer_path, import);
        for (use_path, module_paths) in &module_groups {
            let mut group_resolved = false;
            for module_path in module_paths {
                let mut found = false;
                for candidate in rust_module_file_candidates(module_path) {
                    if candidate == importer_path || !self.file_meta.contains_key(&candidate) {
                        continue;
                    }
                    if seen.insert(candidate.clone()) {
                        deps.push(candidate);
                    }
                    found = true;
                    group_resolved = true;
                    break;
                }
                if found {
                    break;
                }
            }

            if !group_resolved {
                let fallback_import = format!("use {use_path};");
                if let Some(candidate) =
                    self.resolve_generic_import(importer_path, &fallback_import)
                {
                    if seen.insert(candidate.clone()) {
                        deps.push(candidate);
                    }
                }
            }
        }

        if deps.is_empty() && module_groups.is_empty() {
            if let Some(candidate) = self.resolve_generic_import(importer_path, import) {
                deps.push(candidate);
            }
        }

        deps
    }

    fn exact_import_match(&self, importer_path: &str, terms: &[String]) -> Option<String> {
        let mut best_match: Option<(i32, String)> = None;

        for term in terms {
            for (score, candidate) in exact_import_candidates(importer_path, term) {
                if candidate == importer_path || !self.file_meta.contains_key(&candidate) {
                    continue;
                }
                let should_replace = best_match.as_ref().is_none_or(|(best_score, best_path)| {
                    score > *best_score || (score == *best_score && candidate < *best_path)
                });
                if should_replace {
                    best_match = Some((score, candidate));
                }
            }
        }

        best_match.map(|(_, path)| path)
    }

    fn rebuild_dep_graph(&mut self) {
        let outlines: Vec<(String, FileOutline)> = self
            .outlines
            .iter()
            .map(|(path, outline)| (path.clone(), outline.clone()))
            .collect();

        self.dep_graph.clear();
        for (path, outline) in outlines {
            let resolution = self.resolve_imports(&path, &outline.imports, outline.language);
            let unresolved = resolution
                .unresolved
                .into_iter()
                .map(|import| unresolved_import_record(&path, &outline, import))
                .collect();
            self.dep_graph
                .set_resolution(&path, resolution.deps, unresolved);
        }
    }

    pub fn to_snapshot_data(&self) -> snapshot::SnapshotDataRaw {
        snapshot::SnapshotDataRaw {
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
            forward_deps: self
                .dep_graph
                .forward
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        }
    }

    pub fn load_from_snapshot(&mut self, data: snapshot::SnapshotData) {
        let raw = data.into_raw();

        self.outlines.clear();
        self.file_meta.clear();
        self.contents.clear();
        self.content_cache.clear();
        self.symbol_index = SymbolIndex::new();
        self.trigram_index = TrigramIndex::new();
        self.word_index = WordIndex::new();
        self.dep_graph.clear();
        self.store = Store::new();

        for (path, outline) in raw.outlines {
            self.symbol_index.index_file(&outline);
            self.outlines.insert(path, outline);
        }

        for (path, meta) in raw.file_meta {
            self.file_meta.insert(path, meta);
        }

        for (path, content) in raw.contents {
            self.trigram_index.index_file(&path, &content);
            self.word_index.index_file(&path, &content);
            self.content_cache.put(path.clone(), content.clone());
            self.contents.insert(path, content);
        }

        let _ = raw.forward_deps;
        self.rebuild_dep_graph();
    }

    pub fn index_project(&mut self, root: impl AsRef<Path>) -> usize {
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

    fn index_file_meta_only_no_rebuild(&mut self, path: &str, byte_size: u64, modified_ms: u64) {
        self.outlines.remove(path);
        self.contents.remove(path);
        self.content_cache.remove(path);
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

pub fn hash_content(content: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in content.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn unindexed_file_stub(path: &str, meta: &FileMeta) -> String {
    let kind = path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|ext| !ext.is_empty())
        .unwrap_or("unknown");
    format!(
        "unindexed {kind} file: {} bytes\npath: {path}\nmodified_ms: {}\n",
        meta.byte_size, meta.modified_ms
    )
}

fn push_context_symbol_candidate(
    scored: &mut Vec<ScoredContextSymbol>,
    seen: &mut HashSet<(String, String, SymbolKind, u32)>,
    score: i32,
    result: SymbolResult,
) {
    if score <= 0 {
        return;
    }
    let key = (
        result.path.clone(),
        result.symbol.name.clone(),
        result.symbol.kind,
        result.symbol.line_start,
    );
    if let Some(existing) = scored.iter_mut().find(|existing| {
        existing.result.path == key.0
            && existing.result.symbol.name == key.1
            && existing.result.symbol.kind == key.2
            && existing.result.symbol.line_start == key.3
    }) {
        if score > existing.score {
            existing.score = score;
            existing.result = result;
        }
        return;
    }
    if seen.insert(key) {
        scored.push(ScoredContextSymbol { score, result });
    }
}

fn context_keywords(task: &str) -> Vec<String> {
    let mut keywords = Vec::new();
    let mut seen = HashSet::new();

    for quoted in quoted_segments(task) {
        add_context_keyword_variants(&mut keywords, &mut seen, &quoted);
    }

    let tokens = context_tokens(task);
    for token in &tokens {
        if is_identifier_like_context_token(token) {
            add_context_keyword_variants(&mut keywords, &mut seen, token);
        }
    }

    for window_size in 2..=3 {
        for window in tokens.windows(window_size) {
            if window
                .iter()
                .all(|token| context_normalize(token).len() >= 3)
            {
                add_context_phrase_variants(&mut keywords, &mut seen, window);
            }
        }
    }

    if keywords.is_empty() {
        for token in &tokens {
            if context_normalize(token).len() >= 3 {
                add_context_keyword_variants(&mut keywords, &mut seen, token);
            }
        }
    }

    keywords
}

fn context_query_is_explicit(task: &str) -> bool {
    !quoted_segments(task).is_empty()
        || context_tokens(task).into_iter().any(|token| {
            token.contains(['_', '-', '/', '.', ':'])
                || has_lower_to_upper_transition(&token)
                || token.chars().any(|ch| ch.is_ascii_digit())
        })
}

fn quoted_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if !matches!(ch, '"' | '\'' | '`') {
            continue;
        }
        let quote = ch;
        let start = chars.peek().map(|(idx, _)| *idx).unwrap_or(text.len());
        for (end, current) in chars.by_ref() {
            if current == quote {
                if end > start {
                    segments.push(text[start..end].to_string());
                }
                break;
            }
        }
    }
    segments
}

fn context_tokens(task: &str) -> Vec<String> {
    task.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| {
                !c.is_alphanumeric() && !matches!(c, '_' | '-' | '/' | '.' | ':')
            })
            .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

fn add_context_keyword_variants(
    keywords: &mut Vec<String>,
    seen: &mut HashSet<String>,
    keyword: &str,
) {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return;
    }

    push_context_keyword(keywords, seen, keyword.to_string());

    let normalized = context_normalize(keyword);
    if normalized.len() >= 3 {
        push_context_keyword(keywords, seen, normalized.clone());
        if let Some(singular) = singular_context_term(&normalized) {
            push_context_keyword(keywords, seen, singular);
        }
    }

    if keyword.contains(['-', '_', '/', '.', ':']) {
        let separator_normalized = keyword
            .replace(['/', '.', ':'], "-")
            .replace('_', "-")
            .trim_matches('-')
            .to_string();
        if !separator_normalized.is_empty() {
            push_context_keyword(keywords, seen, separator_normalized);
        }
    }
}

fn add_context_phrase_variants(
    keywords: &mut Vec<String>,
    seen: &mut HashSet<String>,
    terms: &[String],
) {
    if terms.is_empty() {
        return;
    }

    let joined_dash = terms.join("-");
    let joined_underscore = terms.join("_");
    let joined_space = terms.join(" ");
    push_context_keyword(keywords, seen, joined_dash.clone());
    push_context_keyword(keywords, seen, joined_underscore);
    push_context_keyword(keywords, seen, joined_space);

    let singular_terms = terms
        .iter()
        .map(|term| singular_context_term(term).unwrap_or_else(|| term.clone()))
        .collect::<Vec<_>>();
    if singular_terms != terms {
        push_context_keyword(keywords, seen, singular_terms.join("-"));
        push_context_keyword(keywords, seen, singular_terms.join("_"));
        push_context_keyword(keywords, seen, singular_terms.join(""));
    }

    let joined_normalized = context_normalize(&joined_dash);
    if joined_normalized.len() >= 3 {
        push_context_keyword(keywords, seen, joined_normalized);
    }
}

fn push_context_keyword(keywords: &mut Vec<String>, seen: &mut HashSet<String>, keyword: String) {
    if keyword.len() < 3 {
        return;
    }
    let key = keyword.to_lowercase();
    if seen.insert(key) {
        keywords.push(keyword);
    }
}

fn is_identifier_like_context_token(token: &str) -> bool {
    token.contains(['_', '-', '/', '.', ':'])
        || has_lower_to_upper_transition(token)
        || token
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic())
            .take(8)
            .count()
            >= 3
            && token.chars().all(|ch| {
                !ch.is_ascii_alphabetic() || ch.is_ascii_uppercase() || ch.is_ascii_digit()
            })
}

fn has_lower_to_upper_transition(token: &str) -> bool {
    let mut previous_lower = false;
    for ch in token.chars() {
        if previous_lower && ch.is_ascii_uppercase() {
            return true;
        }
        previous_lower = ch.is_ascii_lowercase();
    }
    false
}

fn has_identifier_case_signal(token: &str) -> bool {
    has_lower_to_upper_transition(token)
        || token
            .chars()
            .any(|ch| ch.is_ascii_uppercase() || matches!(ch, '_' | '-' | '/' | '.' | ':'))
}

fn context_terms(keywords: &[String]) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    for keyword in keywords {
        let normalized = context_normalize(keyword);
        if normalized.len() >= 3 && seen.insert(normalized.clone()) {
            terms.push(normalized.clone());
        }
        if let Some(singular) = singular_context_term(&normalized) {
            if singular.len() >= 3 && seen.insert(singular.clone()) {
                terms.push(singular);
            }
        }

        for raw_term in keyword.split(|ch: char| !ch.is_alphanumeric()) {
            let term = context_normalize(raw_term);
            if term.len() >= 3 && seen.insert(term.clone()) {
                terms.push(term.clone());
            }
            if let Some(singular) = singular_context_term(&term) {
                if singular.len() >= 3 && seen.insert(singular.clone()) {
                    terms.push(singular);
                }
            }
        }
    }
    terms
}

fn context_core_terms(keywords: &[String]) -> Vec<String> {
    context_terms(keywords)
        .into_iter()
        .filter(|term| {
            (term.len() >= 4 || term == "run")
                && !matches!(
                    term.as_str(),
                    "what"
                        | "when"
                        | "where"
                        | "which"
                        | "with"
                        | "this"
                        | "that"
                        | "from"
                        | "into"
                        | "does"
                        | "work"
                        | "works"
                        | "look"
                        | "find"
                        | "show"
                        | "application"
                )
        })
        .collect()
}

fn context_normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn singular_context_term(term: &str) -> Option<String> {
    if term.len() > 3 && term.ends_with('s') {
        Some(term.trim_end_matches('s').to_string())
    } else {
        None
    }
}

fn symbol_kind_context_score(kind: SymbolKind) -> i32 {
    match kind {
        SymbolKind::Function | SymbolKind::Method => 70,
        SymbolKind::StructDef
        | SymbolKind::ClassDef
        | SymbolKind::InterfaceDef
        | SymbolKind::TraitDef
        | SymbolKind::ImplBlock
        | SymbolKind::UnionDef
        | SymbolKind::TypeAlias => 55,
        SymbolKind::EnumDef | SymbolKind::Module | SymbolKind::MacroDef => 40,
        SymbolKind::Constant | SymbolKind::Variable => 25,
        SymbolKind::TestDecl => 10,
        SymbolKind::CommentBlock => -20,
        SymbolKind::Import => -100,
    }
}

fn is_test_like_path(path: &str) -> bool {
    path.contains("/test/")
        || path.contains("/tests/")
        || path.contains("__tests__")
        || path.ends_with("_test.rs")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.tsx")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn matches_path_glob(pattern: &str, path: &str) -> bool {
    if match_glob(pattern, path) {
        return true;
    }
    if !pattern.contains('/') {
        return match_glob(&format!("**/{pattern}"), path);
    }
    false
}

pub fn is_comment_or_blank(line: &str, language: Language) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }

    match language {
        Language::Python
        | Language::Ruby
        | Language::R
        | Language::Shell
        | Language::Hcl
        | Language::Yaml => trimmed.starts_with('#'),
        Language::Sql => trimmed.starts_with("--"),
        Language::Css | Language::Scss => {
            trimmed.starts_with("/*") || trimmed.starts_with('*') || trimmed.ends_with("*/")
        }
        Language::Markdown => trimmed.starts_with("<!--"),
        _ => {
            trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
                || trimmed.ends_with("*/")
        }
    }
}

fn import_terms(import: &str) -> Vec<String> {
    let raw = import.trim().trim_end_matches(';').trim();
    let mut terms = Vec::new();

    if let Some(quoted) = extract_quoted(raw) {
        terms.push(quoted);
    } else if let Some(included) = extract_include(raw) {
        terms.push(included);
    } else if let Some(rest) = raw.strip_prefix("from ") {
        if let Some((module, _)) = rest.split_once(" import ") {
            terms.push(module.trim().replace('.', "/"));
        }
    } else if let Some(rest) = raw.strip_prefix("import ") {
        if let Some(module) = rest.split(|c: char| c == ',' || c.is_whitespace()).next() {
            terms.push(module.trim().replace('.', "/"));
        }
    } else if let Some(rest) = raw.strip_prefix("use ") {
        terms.extend(expand_rust_use_terms(rest));
    }

    if terms.is_empty() {
        terms.push(raw.to_string());
    }

    let mut expanded = Vec::new();
    for term in terms {
        let normalized = normalize_import_term(&term);
        if normalized.is_empty() {
            continue;
        }
        expanded.push(normalized.clone());
        if let Some(last) = normalized.rsplit('/').next() {
            expanded.push(last.to_string());
        }
    }

    expanded.sort();
    expanded.dedup();
    expanded
}

fn is_local_generic_import(terms: &[String]) -> bool {
    terms
        .iter()
        .any(|term| term.starts_with("./") || term.starts_with("../"))
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

fn rust_import_module_path_groups(importer_path: &str, import: &str) -> Vec<(String, Vec<String>)> {
    let Some(use_tree) = rust_use_tree(import) else {
        return Vec::new();
    };
    let (source_root, importer_module) = rust_source_root_and_module_path(importer_path);
    let expanded_paths = expand_rust_use_tree(use_tree);
    let mut groups = Vec::new();

    for use_path in expanded_paths {
        let module_paths =
            rust_module_paths_from_use_path(&source_root, &importer_module, &use_path);
        if !module_paths.is_empty() {
            groups.push((use_path, module_paths));
        }
    }

    groups
}

fn rust_use_tree(import: &str) -> Option<&str> {
    let raw = import.trim().trim_end_matches(';').trim();

    for (idx, _) in raw.match_indices("use") {
        let before = raw[..idx].chars().next_back();
        let after = raw[idx + "use".len()..].chars().next();
        let before_is_boundary = before.is_none_or(|ch| !is_rust_ident_char(ch));
        let after_is_boundary = after.is_some_and(char::is_whitespace);
        if before_is_boundary && after_is_boundary {
            return Some(raw[idx + "use".len()..].trim());
        }
    }

    None
}

fn is_rust_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn expand_rust_use_tree(use_tree: &str) -> Vec<String> {
    let use_tree = use_tree.trim();
    let Some((start, end)) = top_level_brace_pair(use_tree) else {
        return vec![use_tree.to_string()];
    };

    let prefix = use_tree[..start].trim();
    let suffix = use_tree[end + 1..].trim();
    let inner = &use_tree[start + 1..end];
    let mut paths = Vec::new();

    for item in split_top_level_commas(inner) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let combined = format!("{prefix}{item}{suffix}");
        paths.extend(expand_rust_use_tree(&combined));
    }

    paths
}

fn top_level_brace_pair(value: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut depth = 0usize;

    for (idx, ch) in value.char_indices() {
        match ch {
            '{' => {
                if depth == 0 && start.is_none() {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return start.map(|start| (start, idx));
                }
            }
            _ => {}
        }
    }

    None
}

fn split_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;

    for (idx, ch) in value.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&value[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&value[start..]);
    parts
}

fn rust_source_root_and_module_path(path: &str) -> (String, Vec<String>) {
    let (source_root, relative) =
        if let Some((source_root, relative)) = rust_bin_source_root_and_relative(path) {
            (source_root, relative)
        } else if let Some(relative) = path.strip_prefix("src/") {
            ("src".to_string(), relative)
        } else if let Some((prefix, relative)) = path.rsplit_once("/src/") {
            (format!("{prefix}/src"), relative)
        } else if let Some((dir, filename)) = path.rsplit_once('/') {
            (dir.to_string(), filename)
        } else {
            (String::new(), path)
        };

    let module = if relative == "lib.rs" || relative == "main.rs" {
        ""
    } else if let Some(module) = relative.strip_suffix("/mod.rs") {
        module
    } else {
        strip_known_extension(relative)
    };

    let segments = module
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect();

    (source_root, segments)
}

fn rust_bin_source_root_and_relative(path: &str) -> Option<(String, &str)> {
    let (src_prefix, bin_relative) = if let Some(relative) = path.strip_prefix("src/bin/") {
        ("src", relative)
    } else if let Some((prefix, relative)) = path.rsplit_once("/src/bin/") {
        (&path[..prefix.len() + "/src".len()], relative)
    } else {
        return None;
    };

    if let Some((target_name, target_relative)) = bin_relative.split_once('/') {
        return Some((format!("{src_prefix}/bin/{target_name}"), target_relative));
    }

    let target_name = strip_known_extension(bin_relative);
    if target_name == bin_relative {
        None
    } else {
        Some((format!("{src_prefix}/bin/{target_name}"), "main.rs"))
    }
}

fn rust_module_paths_from_use_path(
    source_root: &str,
    importer_module: &[String],
    use_path: &str,
) -> Vec<String> {
    let path = use_path
        .split(" as ")
        .next()
        .unwrap_or(use_path)
        .trim()
        .trim_end_matches("::*")
        .trim_end_matches("::self");
    let segments: Vec<&str> = path
        .split("::")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return Vec::new();
    }

    let mut base: Vec<String> = Vec::new();
    let mut index = 0usize;
    match segments[0] {
        "crate" => index = 1,
        "self" => {
            base.extend(importer_module.iter().cloned());
            index = 1;
        }
        "super" => {
            base.extend(importer_module.iter().cloned());
            while segments
                .get(index)
                .is_some_and(|segment| *segment == "super")
            {
                base.pop();
                index += 1;
            }
        }
        _ => {}
    }

    for segment in &segments[index..] {
        if *segment == "self" || *segment == "*" {
            continue;
        }
        base.push((*segment).to_string());
    }

    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for len in (1..=base.len()).rev() {
        let module_path = base[..len].join("/");
        let full_path = if source_root.is_empty() {
            module_path
        } else {
            format!("{source_root}/{module_path}")
        };
        if seen.insert(full_path.clone()) {
            paths.push(full_path);
        }
    }

    paths
}

fn rust_module_file_candidates(module_path: &str) -> Vec<String> {
    vec![format!("{module_path}.rs"), format!("{module_path}/mod.rs")]
}

fn expand_rust_use_terms(rest: &str) -> Vec<String> {
    let rest = rest.trim().trim_end_matches(';').trim();
    let rest = rest
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::");

    if let Some((prefix, group)) = rest.split_once("::{") {
        let group = group.trim_end_matches('}').trim();
        let prefix = prefix.trim();
        let mut terms = vec![prefix.to_string()];
        let base = prefix.rsplit("::").next().unwrap_or(prefix);
        terms.push(base.to_string());
        for item in group
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if item == "self" {
                continue;
            }
            terms.push(format!("{prefix}::{item}"));
            if let Some((module_path, _name)) = item.rsplit_once("::") {
                terms.push(format!("{prefix}::{module_path}"));
                terms.push(module_path.to_string());
            }
            terms.push(item.to_string());
        }
        return terms;
    }

    if rest.starts_with('{') && rest.ends_with('}') {
        return rest
            .trim_start_matches('{')
            .trim_end_matches('}')
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect();
    }

    let mut terms = vec![rest.to_string()];
    if let Some((module_path, _name)) = rest.rsplit_once("::") {
        terms.push(module_path.to_string());
    }
    terms
}

fn extract_quoted(raw: &str) -> Option<String> {
    let start = raw.find(['"', '\''])?;
    let quote = raw.as_bytes()[start] as char;
    let rest = &raw[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn extract_include(raw: &str) -> Option<String> {
    let start = raw.find('<')?;
    let rest = &raw[start + 1..];
    let end = rest.find('>')?;
    Some(rest[..end].to_string())
}

fn normalize_filter_prefix(prefix: &str) -> String {
    prefix.trim_matches('/').to_string()
}

fn normalize_import_term(term: &str) -> String {
    let normalized = term
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("include ")
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .trim_matches('{')
        .trim_matches('}')
        .replace("::", "/");

    if normalized.starts_with("./") || normalized.starts_with("../") {
        strip_import_resource_suffix(&normalized).to_string()
    } else {
        normalized.replace('.', "/")
    }
}

fn strip_import_resource_suffix(term: &str) -> &str {
    let query_index = term.find('?');
    let hash_index = term.find('#');
    match (query_index, hash_index) {
        (Some(query), Some(hash)) => &term[..query.min(hash)],
        (Some(index), None) | (None, Some(index)) => &term[..index],
        (None, None) => term,
    }
}

fn exact_import_candidates(importer_path: &str, term: &str) -> Vec<(i32, String)> {
    let mut bases = Vec::new();
    let mut seen_bases = HashSet::new();

    if let Some(relative) = resolve_relative_import_base(importer_path, term) {
        push_unique(&mut bases, &mut seen_bases, relative);
    } else {
        let normalized = term.trim_matches('/').to_string();
        if !normalized.is_empty() {
            push_unique(&mut bases, &mut seen_bases, normalized.clone());

            if let Some(dir) = importer_path.rsplit_once('/').map(|(dir, _)| dir) {
                push_unique(&mut bases, &mut seen_bases, format!("{dir}/{normalized}"));
            }

            if !normalized.starts_with("src/") {
                push_unique(&mut bases, &mut seen_bases, format!("src/{normalized}"));
            }
        }
    }

    let specificity = import_term_specificity(term);
    let mut candidates = Vec::new();
    let mut seen_candidates = HashSet::new();

    for base in bases {
        push_scored_candidate(
            &mut candidates,
            &mut seen_candidates,
            1200 + specificity,
            base.clone(),
        );
        push_typescript_source_extension_candidates(
            &mut candidates,
            &mut seen_candidates,
            1190 + specificity,
            &base,
        );
        for ext in IMPORT_FILE_EXTENSIONS {
            push_scored_candidate(
                &mut candidates,
                &mut seen_candidates,
                1100 + specificity,
                format!("{base}.{ext}"),
            );
        }
        for index_file in IMPORT_INDEX_FILES {
            push_scored_candidate(
                &mut candidates,
                &mut seen_candidates,
                1000 + specificity,
                format!("{base}/{index_file}"),
            );
        }
    }

    candidates
}

fn resolve_relative_import_base(importer_path: &str, term: &str) -> Option<String> {
    if !term.starts_with("./") && !term.starts_with("../") {
        return None;
    }

    let mut parts: Vec<&str> = importer_path
        .rsplit_once('/')
        .map(|(dir, _)| dir.split('/').filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();

    for part in term.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn push_unique(values: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    if seen.insert(value.clone()) {
        values.push(value);
    }
}

fn push_scored_candidate(
    values: &mut Vec<(i32, String)>,
    seen: &mut HashSet<String>,
    score: i32,
    value: String,
) {
    if seen.insert(value.clone()) {
        values.push((score, value));
    }
}

fn import_term_specificity(term: &str) -> i32 {
    (term.matches('/').count() as i32 * 50) + term.len().min(80) as i32
}

fn import_match_score(term: &str, path: &str) -> Option<i32> {
    let term = term.trim_matches('/');
    if term.is_empty() {
        return None;
    }

    let path_stem = strip_known_extension(path);
    let specificity = import_term_specificity(term);

    if path == term {
        return Some(1000 + specificity);
    }
    if path_stem == term {
        return Some(950 + specificity);
    }
    if path.ends_with(&format!("/{term}")) {
        return Some(800 + specificity);
    }
    if path_stem.ends_with(&format!("/{term}")) {
        return Some(750 + specificity);
    }
    for ext in IMPORT_FILE_EXTENSIONS {
        if path.ends_with(&format!("/{term}.{ext}")) || path == format!("{term}.{ext}") {
            return Some(700 + specificity);
        }
    }
    for index_file in IMPORT_INDEX_FILES {
        if path.ends_with(&format!("/{term}/{index_file}"))
            || path == format!("{term}/{index_file}")
        {
            return Some(650 + specificity);
        }
    }

    None
}

const IMPORT_FILE_EXTENSIONS: &[&str] = &[
    "rs", "py", "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "go", "java", "rb", "php",
    "zig", "c", "h", "cpp", "hpp", "cc", "hh", "cxx", "hxx",
];

const IMPORT_INDEX_FILES: &[&str] = &[
    "mod.rs",
    "index.ts",
    "index.tsx",
    "index.js",
    "index.jsx",
    "__init__.py",
];

fn strip_known_extension(path: &str) -> &str {
    for ext in IMPORT_FILE_EXTENSIONS {
        if let Some(stem) = path.strip_suffix(&format!(".{ext}")) {
            return stem;
        }
    }
    path
}

fn push_typescript_source_extension_candidates(
    candidates: &mut Vec<(i32, String)>,
    seen: &mut HashSet<String>,
    score: i32,
    base: &str,
) {
    let Some((stem, source_exts)) = typescript_source_extensions_for_runtime_import(base) else {
        return;
    };
    for (index, ext) in source_exts.iter().enumerate() {
        push_scored_candidate(
            candidates,
            seen,
            score - index as i32,
            format!("{stem}.{ext}"),
        );
    }
}

fn typescript_source_extensions_for_runtime_import(
    base: &str,
) -> Option<(&str, &'static [&'static str])> {
    if let Some(stem) = base.strip_suffix(".js") {
        return Some((stem, &["ts", "tsx"]));
    }
    if let Some(stem) = base.strip_suffix(".jsx") {
        return Some((stem, &["tsx"]));
    }
    if let Some(stem) = base.strip_suffix(".mjs") {
        return Some((stem, &["mts"]));
    }
    if let Some(stem) = base.strip_suffix(".cjs") {
        return Some((stem, &["cts"]));
    }
    None
}

fn fuzzy_match(pattern: &[char], text: &str) -> Option<f32> {
    if pattern.is_empty() {
        return Some(0.0);
    }

    let text_chars: Vec<char> = text.chars().collect();
    let mut pattern_idx = 0;
    let mut score = 0.0;
    let mut consecutive_bonus = 0.0;
    let mut last_match_idx = usize::MAX;

    for (text_idx, &ch) in text_chars.iter().enumerate() {
        if pattern_idx >= pattern.len() {
            break;
        }

        if ch == pattern[pattern_idx] || ch.to_ascii_lowercase() == pattern[pattern_idx] {
            score += 1.0;

            if last_match_idx != usize::MAX && text_idx == last_match_idx + 1 {
                consecutive_bonus += 2.0;
            } else {
                consecutive_bonus = 0.0;
            }
            score += consecutive_bonus;

            if text_idx == 0
                || text_chars[text_idx - 1] == '/'
                || text_chars[text_idx - 1] == '_'
                || text_chars[text_idx - 1] == '-'
            {
                score += 5.0;
            }

            last_match_idx = text_idx;
            pattern_idx += 1;
        }
    }

    if pattern_idx == pattern.len() {
        Some(score)
    } else {
        None
    }
}

fn fuzzy_match_score(pattern: &str, text: &str) -> Option<f32> {
    let chars = pattern.chars().collect::<Vec<_>>();
    fuzzy_match(&chars, text)
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(16384)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolKind;

    #[test]
    fn snapshot_keeps_all_contents_beyond_cache_capacity() {
        let mut engine = Engine::new(1);
        engine.index_file("a.rs", "fn a() {}\n");
        engine.index_file("b.rs", "fn b() {}\n");
        engine.index_file("c.rs", "fn c() {}\n");

        let data = engine.to_snapshot_data();

        assert_eq!(data.contents.len(), 3);
    }

    #[test]
    fn load_from_snapshot_replaces_existing_engine_state() {
        let mut source = Engine::new(4);
        source.index_file("fresh.rs", "fn fresh() {}\n");
        let data = source.to_snapshot_data();

        let mut engine = Engine::new(4);
        engine.index_file("stale.rs", "fn stale() {}\n");

        engine.load_from_snapshot(snapshot::SnapshotData::from_raw(data));

        assert!(engine
            .find_symbol("fresh")
            .iter()
            .any(|hit| hit.path == "fresh.rs"));
        assert!(engine.find_symbol("stale").is_empty());
        assert!(engine.read_file("stale.rs", None, None).is_none());
        assert!(engine.file_map().iter().all(|(path, _)| path != "stale.rs"));
        assert_eq!(engine.store().current_seq(), 0);
    }

    #[test]
    fn read_file_out_of_range_returns_empty_content() {
        let mut engine = Engine::new(4);
        engine.index_file("a.rs", "one\ntwo\n");

        assert_eq!(
            engine.read_file("a.rs", Some(99), None),
            Some(String::new())
        );
        assert_eq!(
            engine.read_file("a.rs", Some(3), Some(2)),
            Some(String::new())
        );
    }

    #[test]
    fn dependency_graph_rebuilds_after_later_files_are_indexed() {
        let mut engine = Engine::new(4);
        engine.index_file("src/a.rs", "use crate::b;\nfn a() {}\n");
        engine.index_file("src/b.rs", "pub fn b() {}\n");

        assert_eq!(engine.get_depends_on("src/a.rs"), vec!["src/b.rs"]);
    }

    #[test]
    fn dependency_graph_prefers_specific_nested_rust_module_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/root.rs",
            "use crate::api::client::Client;\nfn root() {}\n",
        );
        engine.index_file("src/client.rs", "pub struct WrongClient;\n");
        engine.index_file("src/api/client.rs", "pub struct Client;\n");

        assert_eq!(
            engine.get_depends_on("src/root.rs"),
            vec!["src/api/client.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_grouped_nested_rust_module_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/root.rs",
            "use crate::api::{client::Client};\nfn root() {}\n",
        );
        engine.index_file("src/client.rs", "pub struct WrongClient;\n");
        engine.index_file("src/api/client.rs", "pub struct Client;\n");

        assert_eq!(
            engine.get_depends_on("src/root.rs"),
            vec!["src/api/client.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_multiple_grouped_rust_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/root.rs",
            "use crate::api::{client::Client, server::Server};\nfn root() {}\n",
        );
        engine.index_file("src/api/client.rs", "pub struct Client;\n");
        engine.index_file("src/api/server.rs", "pub struct Server;\n");

        assert_eq!(
            engine.get_depends_on("src/root.rs"),
            vec!["src/api/client.rs", "src/api/server.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_multiline_grouped_rust_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/root.rs",
            "use crate::{\n    api::client::Client,\n    api::server::Server,\n};\nfn root() {}\n",
        );
        engine.index_file("src/api/client.rs", "pub struct Client;\n");
        engine.index_file("src/api/server.rs", "pub struct Server;\n");

        assert_eq!(
            engine.get_depends_on("src/root.rs"),
            vec!["src/api/client.rs", "src/api/server.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_self_and_super_rust_imports() {
        let mut engine = Engine::new(4);
        engine.index_file("src/api/mod.rs", "use self::client::Client;\nfn api() {}\n");
        engine.index_file(
            "src/api/routes.rs",
            "use super::client::Client;\nfn routes() {}\n",
        );
        engine.index_file("src/client.rs", "pub struct WrongClient;\n");
        engine.index_file("src/api/client.rs", "pub struct Client;\n");

        assert_eq!(
            engine.get_depends_on("src/api/mod.rs"),
            vec!["src/api/client.rs"]
        );
        assert_eq!(
            engine.get_depends_on("src/api/routes.rs"),
            vec!["src/api/client.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_rust_imports_inside_nested_crate_roots() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "crates/core/src/root.rs",
            "use crate::client::Client;\nfn root() {}\n",
        );
        engine.index_file("src/client.rs", "pub struct WrongClient;\n");
        engine.index_file("crates/core/src/client.rs", "pub struct Client;\n");

        assert_eq!(
            engine.get_depends_on("crates/core/src/root.rs"),
            vec!["crates/core/src/client.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_rust_bin_target_crate_roots() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/bin/tool.rs",
            "use crate::client::Client;\nfn main() {}\n",
        );
        engine.index_file("src/client.rs", "pub struct WrongClient;\n");
        engine.index_file("src/bin/tool/client.rs", "pub struct Client;\n");

        assert_eq!(
            engine.get_depends_on("src/bin/tool.rs"),
            vec!["src/bin/tool/client.rs"]
        );
    }

    #[test]
    fn dependency_graph_resolves_nested_rust_bin_target_crate_roots() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/bin/tool/main.rs",
            "use crate::client::Client;\nfn main() {}\n",
        );
        engine.index_file("src/client.rs", "pub struct WrongClient;\n");
        engine.index_file("src/bin/tool/client.rs", "pub struct Client;\n");

        assert_eq!(
            engine.get_depends_on("src/bin/tool/main.rs"),
            vec!["src/bin/tool/client.rs"]
        );
    }

    #[test]
    fn dependency_graph_prefers_specific_relative_js_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/app.ts",
            "import { client } from './feature/client';\nclient();\n",
        );
        engine.index_file("src/client.ts", "export const client = () => 'wrong';\n");
        engine.index_file(
            "src/feature/client.ts",
            "export const client = () => 'right';\n",
        );

        assert_eq!(
            engine.get_depends_on("src/app.ts"),
            vec!["src/feature/client.ts"]
        );
    }

    #[test]
    fn dependency_graph_resolves_parent_relative_js_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/feature/app.ts",
            "import { client } from '../client';\nclient();\n",
        );
        engine.index_file(
            "src/feature/client.ts",
            "export const client = () => 'wrong';\n",
        );
        engine.index_file("src/client.ts", "export const client = () => 'right';\n");

        assert_eq!(
            engine.get_depends_on("src/feature/app.ts"),
            vec!["src/client.ts"]
        );
    }

    #[test]
    fn dependency_graph_resolves_typescript_sources_from_esm_js_specifiers() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "packages/email/src/client.ts",
            "import { EmailError } from './errors.js';\nimport type { SendOptions } from './types.js';\n",
        );
        engine.index_file(
            "packages/email/src/errors.ts",
            "export class EmailError extends Error {}\n",
        );
        engine.index_file(
            "packages/email/src/types.ts",
            "export type SendOptions = { to: string };\n",
        );

        assert_eq!(
            engine.get_depends_on("packages/email/src/client.ts"),
            vec![
                "packages/email/src/errors.ts".to_string(),
                "packages/email/src/types.ts".to_string()
            ]
        );
        assert!(engine
            .get_unresolved_imports("packages/email/src/client.ts")
            .is_empty());
    }

    #[test]
    fn dependency_graph_resolves_tsx_sources_from_jsx_specifiers() {
        let mut engine = Engine::new(4);
        engine.index_file("src/app.tsx", "import Component from './component.jsx';\n");
        engine.index_file(
            "src/component.tsx",
            "export default function Component() { return null; }\n",
        );

        assert_eq!(
            engine.get_depends_on("src/app.tsx"),
            vec!["src/component.tsx"]
        );
        assert!(engine.get_unresolved_imports("src/app.tsx").is_empty());
    }

    #[test]
    fn dependency_graph_resolves_mts_and_cts_sources_from_runtime_specifiers() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/app.ts",
            "import { esm } from './esm.mjs';\nimport { cjs } from './cjs.cjs';\n",
        );
        engine.index_file("src/esm.mts", "export const esm = true;\n");
        engine.index_file("src/cjs.cts", "export const cjs = true;\n");

        assert_eq!(
            engine.get_depends_on("src/app.ts"),
            vec!["src/cjs.cts".to_string(), "src/esm.mts".to_string()]
        );
        assert!(engine.get_unresolved_imports("src/app.ts").is_empty());
    }

    #[test]
    fn dependency_graph_resolves_local_vite_query_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "apps/admin/src/routes/__root.tsx",
            "import appCss from '../styles.css?url';\n",
        );
        engine.index_file("apps/admin/src/styles.css", "body { color: black; }\n");

        assert_eq!(
            engine.get_depends_on("apps/admin/src/routes/__root.tsx"),
            vec!["apps/admin/src/styles.css"]
        );
        assert!(engine
            .get_unresolved_imports("apps/admin/src/routes/__root.tsx")
            .is_empty());
    }

    #[test]
    fn dependency_graph_prefers_real_js_file_over_typescript_source_fallback() {
        let mut engine = Engine::new(4);
        engine.index_file("src/app.ts", "import { runtime } from './runtime.js';\n");
        engine.index_file("src/runtime.js", "export const runtime = 'js';\n");
        engine.index_file("src/runtime.ts", "export const runtime = 'ts';\n");

        assert_eq!(engine.get_depends_on("src/app.ts"), vec!["src/runtime.js"]);
    }

    #[test]
    fn dependency_graph_does_not_fuzzy_resolve_missing_relative_js_imports() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/app.ts",
            "import { selectedModel } from './model-settings/selected-model';\nimport { provider } from './model-settings/lib/providers';\n",
        );
        engine.index_file(
            "src/model-settings/lib/selected-model.ts",
            "export const selectedModel = 'moved';\n",
        );
        engine.index_file(
            "src/model-settings/lib/providers.ts",
            "export const provider = 'ok';\n",
        );

        assert_eq!(
            engine.get_depends_on("src/app.ts"),
            vec!["src/model-settings/lib/providers.ts"]
        );
        let unresolved = engine.get_unresolved_imports("src/app.ts");
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].import, "./model-settings/selected-model");
        assert_eq!(unresolved[0].line_start, Some(1));
    }

    #[test]
    fn dependency_graph_resolves_existing_asset_imports_without_text_indexing_binary_assets() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        let assets = src.join("assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(
            src.join("providers.ts"),
            "import pngIcon from './assets/provider.png';\nimport svgIcon from './assets/provider.svg';\nexport const icons = [pngIcon, svgIcon];\n",
        )
        .unwrap();
        std::fs::write(assets.join("provider.png"), [0u8, 1, 2, 3]).unwrap();
        std::fs::write(assets.join("provider.svg"), "<svg></svg>\n").unwrap();

        let mut engine = Engine::new(4);
        engine.index_project(root.path());

        let asset_paths = engine.glob_files("src/assets/**");
        assert_eq!(
            asset_paths,
            vec![
                "src/assets/provider.png".to_string(),
                "src/assets/provider.svg".to_string()
            ]
        );
        assert!(engine.get_unresolved_imports("src/providers.ts").is_empty());
        assert_eq!(
            engine.get_depends_on("src/providers.ts"),
            vec![
                "src/assets/provider.png".to_string(),
                "src/assets/provider.svg".to_string()
            ]
        );
        assert!(engine
            .read_file("src/assets/provider.png", None, None)
            .unwrap()
            .contains("unindexed png file: 4 bytes"));
        assert_eq!(
            engine.read_file("src/assets/provider.svg", None, None),
            Some("<svg></svg>\n".to_string())
        );
    }

    #[test]
    fn brief_prefers_exact_symbol_definitions_over_call_sites() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/caller.ts",
            "import { createProjectAgent } from './agent';\nexport const agent = createProjectAgent();\n",
        );
        engine.index_file(
            "src/agent.ts",
            "export function createProjectAgent() {\n  return { run() {} };\n}\n",
        );

        let details = engine.build_context_details("how does createProjectAgent work", 5);

        assert_eq!(details.relevant_symbols[0].name, "createProjectAgent");
        assert_eq!(details.relevant_symbols[0].path, "src/agent.ts");
        assert!(details.relevant_symbols[0]
            .content
            .contains("function createProjectAgent"));
    }

    #[test]
    fn brief_uses_path_phrases_to_find_hook_definitions() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/use-terminal-session.ts",
            "export function useTerminalSession() {\n  return { status: 'ready' };\n}\n",
        );
        engine.index_file(
            "src/types.ts",
            "export type TerminalSession = { status: string };\n",
        );
        engine.index_file(
            "src/terminal-pane.tsx",
            "import { useTerminalSession } from './use-terminal-session';\nexport function TerminalPane() {\n  return useTerminalSession().status;\n}\n",
        );

        let details = engine.build_context_details("what does the terminal session do", 5);

        assert_eq!(details.relevant_symbols[0].name, "useTerminalSession");
        assert_eq!(
            details.relevant_symbols[0].path,
            "src/use-terminal-session.ts"
        );
    }

    #[test]
    fn brief_uses_package_paths_and_plural_phrases_to_find_project_agent_definitions() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "packages/agents/src/index.ts",
            "export function createProjectAgent() {\n  return { kind: 'project' };\n}\nexport type ProjectAgent = ReturnType<typeof createProjectAgent>;\n",
        );
        engine.index_file(
            "packages/agents/src/mcp-settings-codec.ts",
            "export function encodeMcpSettings() {\n  return 'settings';\n}\n",
        );

        let details =
            engine.build_context_details("how do project agents work in packages/agents", 5);

        assert_eq!(details.relevant_symbols[0].name, "createProjectAgent");
        assert_eq!(
            details.relevant_symbols[0].path,
            "packages/agents/src/index.ts"
        );
    }

    #[test]
    fn brief_marks_vague_natural_language_as_low_confidence() {
        let engine = Engine::new(4);

        let details = engine.build_context_details("how does this system work", 5);

        assert_eq!(details.confidence, "low");
        assert!(details
            .note
            .as_deref()
            .unwrap()
            .contains("not natural-language QA"));
        assert!(details
            .suggested_next_steps
            .iter()
            .any(|step| step.contains("symbol-search")));
    }

    #[test]
    fn fuzzy_symbols_finds_partial_symbol_names() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/runtime.ts",
            "export function createAgentRuntimeForRun() {}\nexport function createProjectAgent() {}\n",
        );

        let results = engine.fuzzy_symbols("createAgent", 5);

        assert!(results
            .iter()
            .any(|result| result.name == "createAgentRuntimeForRun"));
        assert!(results
            .iter()
            .any(|result| result.name == "createProjectAgent"));
    }

    #[test]
    fn brief_bounds_large_symbol_bodies() {
        let mut engine = Engine::new(4);
        let mut content = String::from("export function useTerminalSession() {\n");
        for idx in 0..200 {
            content.push_str(&format!("  const value{idx} = {idx};\n"));
        }
        content.push_str("  return value199;\n}\n");
        engine.index_file("src/use-terminal-session.ts", &content);

        let details = engine.build_context_details("useTerminalSession", 5);
        let symbol = &details.relevant_symbols[0];

        assert_eq!(symbol.name, "useTerminalSession");
        assert_eq!(
            symbol.content.lines().count(),
            MAX_CONTEXT_SYMBOL_LINES as usize
        );
        assert!(symbol.content.contains("const value118"));
        assert!(!symbol.content.contains("const value180"));
    }

    #[test]
    fn filtered_files_supports_path_language_line_and_limit_filters() {
        let mut engine = Engine::new(4);
        engine.index_file("src/a.ts", "export const a = 1;\n");
        engine.index_file(
            "src/deep/b.ts",
            "export const b = 1;\nexport const c = 2;\n",
        );
        engine.index_file("src/readme.md", "# Readme\n\nbody\n");
        engine.index_file("packages/pkg/index.ts", "export const pkg = 1;\n");

        let (files, total, truncated) = engine.filtered_files(&FileFilterOptions {
            path_prefix: Some("src".to_string()),
            path_glob: Some("**/*.ts".to_string()),
            language: Some("typescript".to_string()),
            min_lines: Some(1),
            max_lines: Some(2),
            max_results: Some(1),
        });

        assert_eq!(total, 2);
        assert!(truncated);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "src/a.ts");
    }

    #[test]
    fn project_index_rebuilds_dependency_graph_once_after_batch() {
        let root = std::env::temp_dir().join(format!(
            "lexa-engine-test-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.rs"), "use crate::b;\nfn a() {}\n").unwrap();
        std::fs::write(src.join("b.rs"), "pub fn b() {}\n").unwrap();

        let mut engine = Engine::new(4);
        let count = engine.index_project(&root);

        assert_eq!(count, 2);
        assert_eq!(engine.get_depends_on("src/a.rs"), vec!["src/b.rs"]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn python_outline_includes_class_methods() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "service.py",
            "class Service:\n    def handle(self):\n        pass\n",
        );

        let outline = engine.get_outline("service.py").unwrap();
        assert!(outline
            .symbols
            .iter()
            .any(|symbol| symbol.name == "handle" && symbol.kind == SymbolKind::Method));
    }
}
