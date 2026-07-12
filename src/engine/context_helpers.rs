use crate::types::*;
use hashbrown::HashSet;

use super::core::{ContextOptions, ContextSymbol};
use super::ranking::*;
use super::shared::*;

pub(super) struct ScoredSearchResult {
    pub(super) score: i32,
    pub(super) result: SearchResult,
}

pub(super) struct ScoredContextSymbol {
    pub(super) score: i32,
    pub(super) result: SymbolResult,
}

pub(super) fn push_context_symbol_candidate(
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

pub(super) fn suppress_test_context_symbols(
    scored: &mut Vec<ScoredContextSymbol>,
    allow_test_context: bool,
) {
    if allow_test_context
        || !scored
            .iter()
            .any(|entry| !is_test_like_path(&entry.result.path))
    {
        return;
    }
    scored.retain(|entry| !is_test_like_path(&entry.result.path));
}

pub(super) fn suppress_test_context_snippets(
    scored: &mut Vec<ScoredSearchResult>,
    allow_test_context: bool,
) {
    if allow_test_context
        || !scored
            .iter()
            .any(|entry| !is_test_like_path(&entry.result.path))
    {
        return;
    }
    scored.retain(|entry| !is_test_like_path(&entry.result.path));
}

pub(super) fn context_keywords(task: &str) -> Vec<String> {
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

    for token in &tokens {
        if is_context_content_token(token) {
            add_context_keyword_variants(&mut keywords, &mut seen, token);
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

pub(super) fn context_query_is_explicit(task: &str) -> bool {
    !quoted_segments(task).is_empty()
        || context_tokens(task).into_iter().any(|token| {
            token.contains(['_', '-', '/', '.', ':'])
                || has_lower_to_upper_transition(&token)
                || token.chars().any(|ch| ch.is_ascii_digit())
        })
}

pub(super) fn context_confidence(
    task: &str,
    relevant_symbols: &[ContextSymbol],
    snippets: &[SearchResult],
) -> &'static str {
    let has_source_symbol = relevant_symbols
        .iter()
        .any(|symbol| is_source_context_path(&symbol.path));
    let has_source_snippet = snippets
        .iter()
        .any(|snippet| is_source_context_path(&snippet.path));

    if context_query_is_explicit(task) && has_source_symbol {
        "high"
    } else if has_source_symbol || has_source_snippet {
        "medium"
    } else {
        "low"
    }
}

pub(super) fn context_allows_test_context(task: &str, options: &ContextOptions) -> bool {
    context_task_mentions_test_context(task)
        || options
            .path_prefix
            .as_deref()
            .is_some_and(context_filter_targets_test_context)
        || options
            .path_glob
            .as_deref()
            .is_some_and(context_filter_targets_test_context)
}

pub(super) fn context_task_mentions_test_context(task: &str) -> bool {
    context_tokens(task).into_iter().any(|token| {
        matches!(
            context_normalize(&token).as_str(),
            "test" | "tests" | "testing" | "spec" | "specs"
        )
    })
}

pub(super) fn context_filter_targets_test_context(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    let normalized = lowered.trim_matches('/');
    is_test_like_path(normalized)
        || normalized.split('/').any(|segment| {
            let segment =
                segment.trim_matches(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'));
            matches!(
                segment,
                "test" | "tests" | "__tests__" | "spec" | "specs" | "__specs__"
            )
        })
}

pub(super) fn is_context_content_token(token: &str) -> bool {
    let normalized = context_normalize(token);
    normalized.len() >= 3 && !is_low_signal_context_term(&normalized)
}

pub(super) fn quoted_segments(text: &str) -> Vec<String> {
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

pub(super) fn context_tokens(task: &str) -> Vec<String> {
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

pub(super) fn add_context_keyword_variants(
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

pub(super) fn add_context_phrase_variants(
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

pub(super) fn push_context_keyword(
    keywords: &mut Vec<String>,
    seen: &mut HashSet<String>,
    keyword: String,
) {
    if keyword.len() < 3 {
        return;
    }
    let key = keyword.to_lowercase();
    if seen.insert(key) {
        keywords.push(keyword);
    }
}

pub(super) fn is_identifier_like_context_token(token: &str) -> bool {
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

pub(super) fn has_lower_to_upper_transition(token: &str) -> bool {
    let mut previous_lower = false;
    for ch in token.chars() {
        if previous_lower && ch.is_ascii_uppercase() {
            return true;
        }
        previous_lower = ch.is_ascii_lowercase();
    }
    false
}

pub(super) fn has_identifier_case_signal(token: &str) -> bool {
    has_lower_to_upper_transition(token)
        || token
            .chars()
            .any(|ch| ch.is_ascii_uppercase() || matches!(ch, '_' | '-' | '/' | '.' | ':'))
}

pub(super) fn context_terms(keywords: &[String]) -> Vec<String> {
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

pub(super) fn context_core_terms(keywords: &[String]) -> Vec<String> {
    context_terms(keywords)
        .into_iter()
        .filter(|term| (term.len() >= 4 || term == "run") && !is_low_signal_context_term(term))
        .collect()
}

pub(super) fn is_low_signal_context_term(term: &str) -> bool {
    matches!(
        term,
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
            | "how"
            | "why"
            | "the"
            | "and"
            | "for"
            | "application"
    )
}

pub(super) fn context_action_term_score(
    symbol_norm: &str,
    path_norm: &str,
    callable: bool,
    terms: &[String],
) -> i32 {
    let has_action_term = terms
        .iter()
        .any(|term| matches!(term.as_str(), "create" | "build" | "make" | "use" | "run"));
    if !has_action_term {
        return 0;
    }

    if terms
        .iter()
        .filter(|term| matches!(term.as_str(), "create" | "build" | "make" | "use" | "run"))
        .any(|term| symbol_norm.contains(term) || path_norm.contains(term))
    {
        if callable {
            CONTEXT_ACTION_TERM_MATCH_BONUS
        } else {
            CONTEXT_ACTION_TERM_MATCH_BONUS / 2
        }
    } else {
        -CONTEXT_MISSING_ACTION_TERM_PENALTY
    }
}

pub(super) fn singular_context_term(term: &str) -> Option<String> {
    if term.len() > 3 && term.ends_with('s') {
        Some(term.trim_end_matches('s').to_string())
    } else {
        None
    }
}

pub(super) fn is_source_context_path(path: &str) -> bool {
    path.starts_with("src/")
        || (path.starts_with("packages/") && path.contains("/src/"))
        || (path.starts_with("apps/") && path.contains("/src/"))
}

pub(super) fn is_example_context_path(path: &str) -> bool {
    path.starts_with("examples/") || path.contains("/examples/")
}

pub(super) fn context_path_score(path: &str, allow_test_context: bool) -> i32 {
    if is_source_context_path(path) {
        CONTEXT_SOURCE_PATH_BONUS
    } else if is_doc_path(path) {
        -CONTEXT_DOC_PATH_PENALTY
    } else if is_example_context_path(path) {
        -CONTEXT_EXAMPLE_PATH_PENALTY
    } else if is_test_like_path(path) && !allow_test_context {
        -CONTEXT_TEST_PATH_PENALTY
    } else {
        0
    }
}

pub(super) fn context_path_rank(path: &str) -> u8 {
    if is_source_context_path(path) {
        0
    } else if is_test_like_path(path) {
        4
    } else if is_example_context_path(path) {
        5
    } else if is_doc_path(path) {
        6
    } else if path.starts_with("packages/") || path.starts_with("apps/") {
        1
    } else {
        3
    }
}
