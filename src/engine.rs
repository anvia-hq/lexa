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

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub max_results: usize,
    pub regex: bool,
    pub scope: bool,
    pub compact: bool,
    pub paths_only: bool,
    pub path_glob: Option<String>,
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
    pub relevant_symbols: Vec<ContextSymbol>,
    pub snippets: Vec<SearchResult>,
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

struct ScoredSearchResult {
    score: i32,
    result: SearchResult,
}

pub struct DepGraph {
    forward: HashMap<String, Vec<String>>,
    reverse: HashMap<String, HashSet<String>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
        }
    }

    pub fn set_deps(&mut self, path: &str, deps: Vec<String>) {
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
    }

    pub fn clear(&mut self) {
        self.forward.clear();
        self.reverse.clear();
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

    pub fn get_tree(&self) -> String {
        let mut output = String::new();
        for (path, meta) in self.file_map() {
            output.push_str(&format!(
                "{:<60} {:>8} {:>6}L {:>4} sym\n",
                path,
                meta.language.as_str(),
                meta.line_count,
                meta.symbol_count
            ));
        }
        output
    }

    pub fn file_map(&self) -> Vec<(String, FileMeta)> {
        let mut entries: Vec<(&String, &FileMeta)> = self.file_meta.iter().collect();
        entries.sort_by_key(|(path, _)| path.as_str());
        entries
            .into_iter()
            .map(|(path, meta)| (path.clone(), meta.clone()))
            .collect()
    }

    pub fn get_imported_by(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_imported_by(path)
    }

    pub fn get_depends_on(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_depends_on(path)
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

    pub fn build_context(&self, task: &str, max_results: usize) -> String {
        let details = self.build_context_details(task, max_results);

        let mut output = String::new();
        output.push_str(&format!("## Context for: {}\n\n", task));

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

    pub fn build_context_details(&self, task: &str, max_results: usize) -> ContextDetails {
        let mut keywords: Vec<String> = task
            .split_whitespace()
            .map(|word| {
                word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                    .to_string()
            })
            .filter(|w| w.len() > 2)
            .collect();
        let base_keywords = keywords.clone();
        for pair in base_keywords.windows(2) {
            if let [left, right] = pair {
                keywords.push(format!("{}_{}", left, right));
            }
        }
        keywords.sort();
        keywords.dedup();

        let mut relevant_symbols: Vec<ContextSymbol> = Vec::new();
        for keyword in &keywords {
            let symbols = self.find_symbol(keyword);
            for result in symbols {
                if relevant_symbols
                    .iter()
                    .any(|x| x.path == result.path && x.name == result.symbol.name)
                {
                    continue;
                }

                if let Some((content_line_start, content_line_end, content)) =
                    self.symbol_source(&result.path, &result.symbol, 2)
                {
                    relevant_symbols.push(ContextSymbol {
                        path: result.path,
                        name: result.symbol.name,
                        kind: result.symbol.kind.to_string(),
                        line_start: result.symbol.line_start,
                        line_end: result.symbol.line_end,
                        detail: result.symbol.detail,
                        content_line_start,
                        content_line_end,
                        content,
                    });
                }
            }
        }
        relevant_symbols.truncate(5);

        let mut snippets = self.ranked_context_snippets(&keywords, &relevant_symbols, max_results);

        ContextDetails {
            task: task.to_string(),
            keywords,
            max_results,
            relevant_symbols,
            snippets: std::mem::take(&mut snippets),
        }
    }

    fn ranked_context_snippets(
        &self,
        keywords: &[String],
        relevant_symbols: &[ContextSymbol],
        max_results: usize,
    ) -> Vec<SearchResult> {
        let mut scored: Vec<ScoredSearchResult> = Vec::new();
        for keyword in keywords {
            let results = self.search(keyword, 8);
            for result in results {
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
            score += 20;
        }
        if result
            .line_text
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .any(|word| word.eq_ignore_ascii_case(keyword))
        {
            score += 30;
        }
        if path_lower.contains(&keyword_lower) {
            score += 15;
        }
        if relevant_symbols
            .iter()
            .any(|symbol| symbol.path == result.path || symbol.name.eq_ignore_ascii_case(keyword))
        {
            score += 40;
        }

        let language = self
            .file_meta
            .get(&result.path)
            .map(|meta| meta.language)
            .unwrap_or_else(|| detect_language(&result.path));
        if is_comment_or_blank(&result.line_text, language) {
            score -= 20;
        }
        if keyword.len() <= 3 {
            score -= 10;
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
        let content = self.content_for(path)?;
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
        let content = self.content_for(path)?;
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

    pub fn symbol_source(
        &self,
        path: &str,
        symbol: &Symbol,
        context_lines: u32,
    ) -> Option<(u32, u32, String)> {
        let outline = self.outlines.get(path)?;
        let start = symbol.line_start.saturating_sub(context_lines).max(1);
        let end = (symbol.line_end + context_lines).min(outline.line_count);
        self.read_file(path, Some(start), Some(end))
            .map(|content| (start, end, content))
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

    fn resolve_imports(&self, path: &str, imports: &[String], _language: Language) -> Vec<String> {
        let mut deps = Vec::new();
        for import in imports {
            let terms = import_terms(import);
            if terms.is_empty() {
                continue;
            }

            for candidate in self.file_meta.keys() {
                if candidate == path {
                    continue;
                }

                if terms
                    .iter()
                    .any(|term| import_matches_path(term, candidate))
                {
                    deps.push(candidate.clone());
                    break;
                }
            }
        }
        deps.sort();
        deps.dedup();
        deps
    }

    fn rebuild_dep_graph(&mut self) {
        let outlines: Vec<(String, Vec<String>, Language)> = self
            .outlines
            .iter()
            .map(|(path, outline)| (path.clone(), outline.imports.clone(), outline.language))
            .collect();

        self.dep_graph.clear();
        for (path, imports, language) in outlines {
            let deps = self.resolve_imports(&path, &imports, language);
            self.dep_graph.set_deps(&path, deps);
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

        for (path, deps) in raw.forward_deps {
            self.dep_graph.set_deps(&path, deps);
        }
    }

    pub fn index_project(&mut self, root: impl AsRef<Path>) -> usize {
        let files = crate::walker::walk_project(root);
        let count = files.len();

        for file in &files {
            self.index_file_with_op(
                &file.path,
                &file.content,
                file.modified_ms,
                Op::Snapshot,
                false,
            );
        }
        self.rebuild_dep_graph();

        count
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

    vec![rest.to_string()]
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

fn normalize_import_term(term: &str) -> String {
    term.trim()
        .trim_start_matches('#')
        .trim_start_matches("include ")
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .trim_start_matches("./")
        .trim_start_matches("../")
        .trim_matches('{')
        .trim_matches('}')
        .replace("::", "/")
        .replace('.', "/")
}

fn import_matches_path(term: &str, path: &str) -> bool {
    let path_stem = path
        .trim_end_matches(".rs")
        .trim_end_matches(".py")
        .trim_end_matches(".ts")
        .trim_end_matches(".tsx")
        .trim_end_matches(".js")
        .trim_end_matches(".jsx")
        .trim_end_matches(".go")
        .trim_end_matches(".java")
        .trim_end_matches(".rb")
        .trim_end_matches(".php")
        .trim_end_matches(".zig");

    path == term
        || path_stem == term
        || path.ends_with(&format!("/{term}"))
        || path_stem.ends_with(&format!("/{term}"))
        || path.ends_with(&format!("/{term}.rs"))
        || path.ends_with(&format!("/{term}.py"))
        || path.ends_with(&format!("/{term}.ts"))
        || path.ends_with(&format!("/{term}.tsx"))
        || path.ends_with(&format!("/{term}.js"))
        || path.ends_with(&format!("/{term}.jsx"))
        || path.ends_with(&format!("/{term}/mod.rs"))
        || path.ends_with(&format!("/{term}/index.ts"))
        || path.ends_with(&format!("/{term}/index.js"))
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
