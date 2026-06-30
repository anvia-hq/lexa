//! Context ranking weights for `brief` / `ContextOptions`.
//!
//! Keep these values grouped by scoring purpose. When changing a value, update
//! the ranking regression tests under `tests/ranking.rs`.

pub(crate) const MAX_CONTEXT_SYMBOL_LINES: u32 = 120;

// Exact-match bonuses
pub(crate) const CONTEXT_EXACT_SYMBOL_BONUS: i32 = 500;
pub(crate) const CONTEXT_CASE_INSENSITIVE_SYMBOL_BONUS: i32 = 450;
pub(crate) const CONTEXT_INDEXED_SYMBOL_SOURCE_BONUS: i32 = 500;
pub(crate) const CONTEXT_OUTLINE_SYMBOL_SOURCE_BONUS: i32 = 450;
pub(crate) const CONTEXT_NORMALIZED_EXACT_BONUS: i32 = 420;
pub(crate) const CONTEXT_NORMALIZED_CONTAINS_BONUS: i32 = 180;
pub(crate) const CONTEXT_REVERSE_CONTAINS_BONUS: i32 = 80;
pub(crate) const CONTEXT_CALLABLE_SUFFIX_BONUS: i32 = 320;

// Path / basename bonuses
pub(crate) const CONTEXT_PATH_KEYWORD_BONUS: i32 = 80;
pub(crate) const CONTEXT_BASENAME_CALLABLE_BONUS: i32 = 520;
pub(crate) const CONTEXT_SOURCE_PATH_BONUS: i32 = 180;
pub(crate) const CONTEXT_DOC_PATH_PENALTY: i32 = 180;
pub(crate) const CONTEXT_EXAMPLE_PATH_PENALTY: i32 = 120;

// Callable / kind biases
pub(crate) const CONTEXT_NONCALLABLE_SHADOW_PENALTY: i32 = 260;
pub(crate) const CONTEXT_SYMBOL_TERM_BONUS: i32 = 35;
pub(crate) const CONTEXT_PATH_TERM_BONUS: i32 = 10;
pub(crate) const CONTEXT_MULTI_TERM_CALLABLE_BONUS: i32 = 520;
pub(crate) const CONTEXT_ACTION_NAME_BONUS: i32 = 260;
pub(crate) const CONTEXT_ACTION_TERM_MATCH_BONUS: i32 = 360;
pub(crate) const CONTEXT_MISSING_ACTION_TERM_PENALTY: i32 = 260;

// Core-match penalties and bonuses
pub(crate) const CONTEXT_NO_CORE_SYMBOL_PENALTY: i32 = 1000;
pub(crate) const CONTEXT_WEAK_CORE_MATCH_PENALTY: i32 = 1200;
pub(crate) const CONTEXT_STRONG_CORE_MATCH_BONUS: i32 = 600;
pub(crate) const CONTEXT_PATH_CORE_MATCH_BONUS: i32 = 200;
pub(crate) const CONTEXT_POOR_CORE_PATH_PENALTY: i32 = 360;
pub(crate) const CONTEXT_SYMBOL_CORE_TERM_BONUS: i32 = 90;
pub(crate) const CONTEXT_PATH_CORE_TERM_BONUS: i32 = 25;
pub(crate) const CONTEXT_TEST_PATH_PENALTY: i32 = 60;

// Multi-term cluster
pub(crate) const CONTEXT_MULTI_TERM_SYMBOL_BONUS: i32 = 120;
pub(crate) const CONTEXT_MULTI_TERM_PATH_BONUS: i32 = 55;
pub(crate) const CONTEXT_MULTI_TERM_CALLABLE_KIND_BONUS: i32 = 120;
pub(crate) const CONTEXT_MULTI_TERM_ACTION_BONUS: i32 = 140;
pub(crate) const CONTEXT_MULTI_TERM_RUNTIME_BONUS: i32 = 80;

// Snippet scoring
pub(crate) const CONTEXT_SNIPPET_SYMBOL_DEFINITION_BONUS: i32 = 260;
pub(crate) const CONTEXT_SNIPPET_SYMBOL_RANK_STEP: i32 = 12;
pub(crate) const CONTEXT_SNIPPET_LINE_MATCH_BONUS: i32 = 20;
pub(crate) const CONTEXT_SNIPPET_WORD_MATCH_BONUS: i32 = 30;
pub(crate) const CONTEXT_SNIPPET_PATH_MATCH_BONUS: i32 = 15;
pub(crate) const CONTEXT_SNIPPET_RELEVANT_SYMBOL_BONUS: i32 = 90;
pub(crate) const CONTEXT_SNIPPET_TOP_SYMBOL_FILE_BONUS: i32 = 70;
pub(crate) const CONTEXT_SNIPPET_SOURCE_PATH_BONUS: i32 = 20;
pub(crate) const CONTEXT_SNIPPET_DOC_PATH_PENALTY: i32 = 30;
pub(crate) const CONTEXT_SNIPPET_EXAMPLE_PATH_PENALTY: i32 = 20;
pub(crate) const CONTEXT_SNIPPET_TEST_PATH_PENALTY: i32 = 80;
pub(crate) const CONTEXT_SNIPPET_COMMENT_PENALTY: i32 = 20;
pub(crate) const CONTEXT_SNIPPET_IMPORT_PENALTY: i32 = 35;
pub(crate) const CONTEXT_SNIPPET_SHORT_KEYWORD_PENALTY: i32 = 10;
