use crate::types::{Language, SymbolKind};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn context_normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(super) fn symbol_kind_context_score(kind: SymbolKind) -> i32 {
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

pub(super) fn is_test_like_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    let file_name = path.rsplit('/').next().unwrap_or(&path);
    path.split('/').any(|segment| {
        matches!(
            segment,
            "test" | "tests" | "__tests__" | "spec" | "specs" | "__specs__"
        )
    }) || file_name.ends_with("_test.rs")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("import ")
        || trimmed.starts_with("import type ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("from ")
}

pub(super) fn normalize_filter_prefix(prefix: &str) -> String {
    prefix.trim_matches('/').to_string()
}

pub(super) fn is_doc_path(path: &str) -> bool {
    path.starts_with("docs/")
        || path.eq_ignore_ascii_case("readme.md")
        || path.ends_with(".md")
        || path.ends_with(".mdx")
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

pub(super) fn fuzzy_match(pattern: &[char], text: &str) -> Option<f32> {
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

pub(super) fn fuzzy_match_score(pattern: &str, text: &str) -> Option<f32> {
    let chars = pattern.chars().collect::<Vec<_>>();
    fuzzy_match(&chars, text)
}
