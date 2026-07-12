use crate::types::*;
use hashbrown::HashSet;
use regex::Regex;

use super::core::*;
use super::files::matches_path_glob;
use super::shared::*;

impl Engine {
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
}

impl Engine {
    pub fn search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let words: Vec<&str> = query_lower.split_whitespace().collect();

        if words.len() > 1 {
            return self.search_multi_word(&words, max_results);
        }

        let single_word = words.first().copied().unwrap_or(&query_lower);
        let mut results = Vec::new();
        let mut seen: HashSet<(String, u32)> = HashSet::new();

        let word_hits = self.word_index.search_limited(single_word, max_results);
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
        let mut candidate_set = HashSet::new();

        let word_hits = self.word_index.search(first_word);
        for (path, _line_num) in &word_hits {
            if candidate_set.insert(path.clone()) {
                candidate_paths.push(path.clone());
            }
        }

        let trigram_candidates = self.trigram_index.candidates(first_word);
        for path in trigram_candidates {
            if candidate_set.insert(path.clone()) {
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
}

impl Engine {
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
}

impl Engine {
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

    pub fn search_word_with_options(
        &self,
        word: &str,
        options: &WordSearchOptions,
    ) -> Vec<WordSearchResult> {
        let path_prefix = options
            .path_prefix
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(normalize_filter_prefix);

        let mut results = self
            .search_word(word)
            .into_iter()
            .filter(|result| {
                path_prefix.as_ref().is_none_or(|prefix| {
                    result.path == *prefix || result.path.starts_with(&format!("{prefix}/"))
                }) && options
                    .path_glob
                    .as_deref()
                    .is_none_or(|glob| matches_path_glob(glob, &result.path))
            })
            .map(|result| self.classified_word_result(word, result))
            .collect::<Vec<_>>();

        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| {
                    word_result_path_rank(&left.path).cmp(&word_result_path_rank(&right.path))
                })
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.line_num.cmp(&right.line_num))
        });
        results
    }

    fn classified_word_result(&self, word: &str, result: SearchResult) -> WordSearchResult {
        let kind = self.word_occurrence_kind(word, &result);
        let score = word_occurrence_score(kind, &result.path);
        WordSearchResult {
            path: result.path,
            line_num: result.line_num,
            line_text: result.line_text,
            kind: kind.to_string(),
            score,
        }
    }

    fn word_occurrence_kind(&self, word: &str, result: &SearchResult) -> &'static str {
        if is_doc_path(&result.path) {
            return "doc";
        }

        let semantic = if self.is_word_definition(word, result) {
            "definition"
        } else if is_import_line(&result.line_text) {
            "import"
        } else if is_export_line(&result.line_text) {
            "export"
        } else if is_call_like_occurrence(word, &result.line_text) {
            "call"
        } else {
            "reference"
        };

        if is_test_like_path(&result.path) && matches!(semantic, "call" | "reference") {
            "test"
        } else {
            semantic
        }
    }

    fn is_word_definition(&self, word: &str, result: &SearchResult) -> bool {
        self.outlines.get(&result.path).is_some_and(|outline| {
            outline.symbols.iter().any(|symbol| {
                symbol.name == word
                    && symbol.line_start == result.line_num
                    && !matches!(symbol.kind, SymbolKind::Import | SymbolKind::CommentBlock)
            })
        })
    }
}

fn word_result_path_rank(path: &str) -> u8 {
    if is_test_like_path(path) {
        5
    } else if path.starts_with("packages/") && path.contains("/src/") {
        0
    } else if path.starts_with("apps/") && path.contains("/src/") {
        1
    } else if path.starts_with("src/") {
        2
    } else if path.starts_with("packages/") || path.starts_with("apps/") {
        3
    } else if path.starts_with("examples/") {
        4
    } else if is_doc_path(path) {
        6
    } else {
        4
    }
}

fn word_occurrence_score(kind: &str, path: &str) -> i32 {
    occurrence_kind_score(kind) + word_path_score(path)
}

fn occurrence_kind_score(kind: &str) -> i32 {
    match kind {
        "definition" => 100,
        "export" => 90,
        "import" => 75,
        "call" => 65,
        "reference" => 50,
        "test" => 30,
        "doc" => 10,
        _ => 0,
    }
}

fn word_path_score(path: &str) -> i32 {
    match word_result_path_rank(path) {
        0 => 20,
        1 => 16,
        2 => 14,
        3 => 8,
        4 => 0,
        5 => -40,
        6 => -30,
        _ => 0,
    }
}

fn is_export_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("export ")
}

fn is_call_like_occurrence(word: &str, line: &str) -> bool {
    let patterns = [
        format!("{word}("),
        format!("new {word}("),
        format!("{word}::"),
        format!(".{word}("),
    ];
    patterns.iter().any(|pattern| line.contains(pattern))
}
