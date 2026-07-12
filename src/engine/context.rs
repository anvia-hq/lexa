use crate::glob::match_glob;
use crate::types::*;
use hashbrown::HashSet;

use super::context_helpers::*;
use super::core::*;
use super::ranking::*;
use super::shared::*;

impl Engine {
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
        let allow_test_context = context_allows_test_context(task, options);
        let relevant_symbols =
            self.ranked_context_symbols(&keywords, options, 5, allow_test_context);

        let mut snippets = self.ranked_context_snippets(
            &keywords,
            &relevant_symbols,
            options,
            max_results,
            allow_test_context,
        );
        let confidence = context_confidence(task, &relevant_symbols, &snippets);
        let low_confidence = confidence == "low";

        ContextDetails {
            task: task.to_string(),
            keywords,
            max_results,
            confidence: confidence.to_string(),
            note: low_confidence.then(|| {
                "Low-confidence brief: this tool bundles context from explicit symbols, path fragments, and scoped keywords; it is not natural-language QA.".to_string()
            }),
            suggested_next_steps: if low_confidence {
                vec![
                    "Add --path-prefix or --path-glob to scope the search.".to_string(),
                    "Run symbol-search for likely symbol names.".to_string(),
                    "Run text-search for concrete terms from the task.".to_string(),
                ]
            } else {
                Vec::new()
            },
            relevant_symbols,
            snippets: std::mem::take(&mut snippets),
        }
    }
}

impl Engine {
    fn ranked_context_symbols(
        &self,
        keywords: &[String],
        options: &ContextOptions,
        max_symbols: usize,
        allow_test_context: bool,
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
                    self.context_symbol_score(keyword, &result, keywords, allow_test_context)
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
                            allow_test_context,
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
                    let score =
                        self.context_symbol_score(keyword, &result, keywords, allow_test_context)
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
                    let score = self.context_multi_term_symbol_score(
                        &result,
                        &core_terms,
                        allow_test_context,
                    );
                    if score >= 260 {
                        push_context_symbol_candidate(&mut scored, &mut seen, score, result);
                    }
                }
            }
        }

        suppress_test_context_symbols(&mut scored, allow_test_context);
        scored.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| {
                    context_path_rank(&a.result.path).cmp(&context_path_rank(&b.result.path))
                })
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
}

impl Engine {
    fn context_symbol_score(
        &self,
        keyword: &str,
        result: &SymbolResult,
        keywords: &[String],
        allow_test_context: bool,
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
        score += context_path_score(&result.path, allow_test_context);

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
            score += context_action_term_score(&symbol_norm, &path_norm, callable, &core_terms);
        }

        if is_test_like_path(&result.path) && !allow_test_context {
            score -= CONTEXT_TEST_PATH_PENALTY;
        }

        score
    }

    fn context_multi_term_symbol_score(
        &self,
        result: &SymbolResult,
        terms: &[String],
        allow_test_context: bool,
    ) -> i32 {
        let symbol_norm = context_normalize(&result.symbol.name);
        let path_norm = context_normalize(&result.path);
        let mut matched_symbol_terms = 0;
        let mut matched_path_terms = 0;
        let mut score = symbol_kind_context_score(result.symbol.kind);
        score += context_path_score(&result.path, allow_test_context);

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
        let is_callable = matches!(
            result.symbol.kind,
            SymbolKind::Function | SymbolKind::Method
        );
        if is_callable {
            score += CONTEXT_MULTI_TERM_CALLABLE_KIND_BONUS;
        }
        if symbol_norm.starts_with("create") || symbol_norm.starts_with("run") {
            score += CONTEXT_MULTI_TERM_ACTION_BONUS;
        }
        if symbol_norm.contains("runtime") {
            score += CONTEXT_MULTI_TERM_RUNTIME_BONUS;
        }
        score += context_action_term_score(&symbol_norm, &path_norm, is_callable, terms);
        if is_test_like_path(&result.path) && !allow_test_context {
            score -= CONTEXT_TEST_PATH_PENALTY;
        }
        score
    }
}

impl Engine {
    fn ranked_context_snippets(
        &self,
        keywords: &[String],
        relevant_symbols: &[ContextSymbol],
        options: &ContextOptions,
        max_results: usize,
        allow_test_context: bool,
    ) -> Vec<SearchResult> {
        let mut scored: Vec<ScoredSearchResult> = Vec::new();
        for (rank, symbol) in relevant_symbols.iter().enumerate() {
            if !self.context_path_allowed(&symbol.path, options) {
                continue;
            }
            if scored
                .iter()
                .any(|x| x.result.path == symbol.path && x.result.line_num == symbol.line_start)
            {
                continue;
            }
            let Some(line_text) = self.get_line(&symbol.path, symbol.line_start) else {
                continue;
            };
            let rank_penalty = (rank as i32) * CONTEXT_SNIPPET_SYMBOL_RANK_STEP;
            scored.push(ScoredSearchResult {
                score: CONTEXT_SNIPPET_SYMBOL_DEFINITION_BONUS.saturating_sub(rank_penalty),
                result: SearchResult {
                    path: symbol.path.clone(),
                    line_num: symbol.line_start,
                    line_text,
                },
            });
        }

        for keyword in keywords {
            let per_keyword_limit = if allow_test_context { 8 } else { 24 };
            let results = self.search(keyword, per_keyword_limit);
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
                let score = self.context_snippet_score(
                    keyword,
                    &result,
                    relevant_symbols,
                    allow_test_context,
                );
                scored.push(ScoredSearchResult { score, result });
            }
        }
        suppress_test_context_snippets(&mut scored, allow_test_context);
        scored.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| {
                    context_path_rank(&a.result.path).cmp(&context_path_rank(&b.result.path))
                })
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
}

impl Engine {
    fn context_snippet_score(
        &self,
        keyword: &str,
        result: &SearchResult,
        relevant_symbols: &[ContextSymbol],
        allow_test_context: bool,
    ) -> i32 {
        let keyword_lower = keyword.to_lowercase();
        let path_lower = result.path.to_lowercase();
        let line_lower = result.line_text.to_lowercase();
        let mut score = 0;
        if is_source_context_path(&result.path) {
            score += CONTEXT_SNIPPET_SOURCE_PATH_BONUS;
        } else if is_doc_path(&result.path) {
            score -= CONTEXT_SNIPPET_DOC_PATH_PENALTY;
        } else if is_example_context_path(&result.path) {
            score -= CONTEXT_SNIPPET_EXAMPLE_PATH_PENALTY;
        }
        if is_test_like_path(&result.path) && !allow_test_context {
            score -= CONTEXT_SNIPPET_TEST_PATH_PENALTY;
        }

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
        if relevant_symbols
            .first()
            .is_some_and(|symbol| symbol.path == result.path)
        {
            score += CONTEXT_SNIPPET_TOP_SYMBOL_FILE_BONUS;
        }

        let language = self
            .file_meta
            .get(&result.path)
            .map(|meta| meta.language)
            .unwrap_or_else(|| detect_language(&result.path));
        if is_comment_or_blank(&result.line_text, language) {
            score -= CONTEXT_SNIPPET_COMMENT_PENALTY;
        }
        if is_import_line(&result.line_text) {
            score -= CONTEXT_SNIPPET_IMPORT_PENALTY;
        }
        if keyword.len() <= 3 {
            score -= CONTEXT_SNIPPET_SHORT_KEYWORD_PENALTY;
        }
        score
    }
}
