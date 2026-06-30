//! Canonical MCP tool descriptions.
//!
//! `summary` is the short sentence sent over MCP. `description` keeps the
//! longer guidance used by generated documentation.

use serde::Serialize;
use serde_json::{json, Value};
use std::sync::LazyLock;

#[derive(Debug, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub static TOOL_SPECS: LazyLock<Vec<ToolSpec>> = LazyLock::new(|| {
    vec![
        ToolSpec {
            name: "files",
            summary: "Start here for an overview of the indexed project.",
            description: "Use at the start of exploration to get an overview of the indexed project. Returns indexed file paths with language, line count, and symbol count; supports filtering by path prefix, glob, language, and line-count range. Prefer this over `glob` or `path_search` when you want a broad view rather than a targeted lookup.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string","description":"Optional project-relative path prefix."},"path_glob":{"type":"string"},"language":{"type":"string","description":"Language name such as typescript, rust, json, or markdown."},"min_lines":{"type":"integer"},"max_lines":{"type":"integer"},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."}},"required":[]}),
        },
        ToolSpec {
            name: "list",
            summary: "List immediate children of one directory.",
            description: "Use when you need to see the immediate children of one directory, similar to `ls`. Returns each child name and whether it is a file or directory. Faster than `files` for inspecting a single folder.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":[]}),
        },
        ToolSpec {
            name: "glob",
            summary: "Match indexed paths with an exact glob pattern.",
            description: "Use when you have an exact glob pattern (e.g. `src/**/*.rs`) and want matching indexed paths. Returns up to 200 paths with match count and truncation flag. Prefer over `path_search` when the pattern is precise rather than approximate.",
            input_schema: json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]}),
        },
        ToolSpec {
            name: "path_search",
            summary: "Fuzzy-match indexed file paths.",
            description: "Use when you only know part of a file name and want fuzzy matches. Returns scored file-path matches ordered by relevance with a configurable limit. Use `query` (or aliases `path`/`pattern`/`name`) and `max_results`/`max` (default 20).",
            input_schema: json!({"type":"object","properties":{"query":{"type":"string"},"path":{"type":"string","description":"Alias for query."},"pattern":{"type":"string","description":"Alias for query."},"name":{"type":"string","description":"Alias for query."},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."}},"anyOf":[{"required":["query"]},{"required":["path"]},{"required":["pattern"]},{"required":["name"]}],"required":[]}),
        },
        ToolSpec {
            name: "outline",
            summary: "Get the imports and symbol list of one file.",
            description: "Use before reading a file to understand its structure. Returns the file's language, line count, imports, and full symbol list (kind, name, line range, detail). Also surfaces unresolved local imports to flag broken references.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
        },
        ToolSpec {
            name: "symbol_defs",
            summary: "Find definitions of an exact symbol name.",
            description: "Use when you know the exact name of a function, class, type, or variable and want its precise definition. Returns every matching definition with file path, line range, kind, and detail string. Use `name` (or alias `query`) as the exact match key.",
            input_schema: json!({"type":"object","properties":{"name":{"type":"string"},"query":{"type":"string","description":"Alias for name."}},"anyOf":[{"required":["name"]},{"required":["query"]}],"required":[]}),
        },
        ToolSpec {
            name: "symbol_search",
            summary: "Fuzzy-match symbol names across the project.",
            description: "Use when you only know part of a symbol name and want fuzzy matches across the project (e.g. `createAgent` matching `createProjectAgent`). Returns scored symbol matches with file, line range, kind, and detail; default limit 20.",
            input_schema: json!({"type":"object","properties":{"query":{"type":"string"},"name":{"type":"string","description":"Alias for query."},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."}},"anyOf":[{"required":["query"]},{"required":["name"]}],"required":[]}),
        },
        ToolSpec {
            name: "word_refs",
            summary: "Find every occurrence of an exact identifier.",
            description: "Use when you want occurrences of an exact identifier or word, including definitions, imports, calls, and references. Acts like `grep -w` over the indexed word index. Use `word` (or alias `query`) as the exact token. Results are classified, ranked, and paginated; pass `cursor` from `next_cursor` to continue. Supports `path_prefix`/`path` and `path_glob` filters.",
            input_schema: json!({"type":"object","properties":{"word":{"type":"string"},"query":{"type":"string","description":"Alias for word."},"max_results":{"type":"integer","minimum":1},"max":{"type":"integer","minimum":1,"description":"Alias for max_results."},"cursor":{"type":"integer","minimum":0,"description":"Zero-based result offset for pagination."},"path_prefix":{"type":"string"},"path":{"type":"string","description":"Alias for path_prefix."},"path_glob":{"type":"string"}},"anyOf":[{"required":["word"]},{"required":["query"]}],"required":[]}),
        },
        ToolSpec {
            name: "text_search",
            summary: "Substring or regex search over indexed text.",
            description: "Use as the grep equivalent over indexed text. Supports substring or regex queries with scope (show enclosing symbol), compact (trimmed output), paths-only (`path:line` pairs), and `path_glob` filters. Default limit 20; results include file, line number, and matched text.",
            input_schema: json!({"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer"},"regex":{"type":"boolean"},"scope":{"type":"boolean"},"compact":{"type":"boolean"},"paths_only":{"type":"boolean"},"path_glob":{"type":"string"}},"required":["query"]}),
        },
        ToolSpec {
            name: "callers",
            summary: "Find non-definition call sites of a symbol.",
            description: "Use to find non-definition call sites and usages of a symbol before refactoring. Returns up to 30 results excluding declarations and type aliases, so the list reflects real call impact. Use `name` (or alias `query`) for the exact symbol.",
            input_schema: json!({"type":"object","properties":{"name":{"type":"string"},"query":{"type":"string","description":"Alias for name."}},"anyOf":[{"required":["name"]},{"required":["query"]}],"required":[]}),
        },
        ToolSpec {
            name: "brief",
            summary: "Compose a focused context bundle for a code task.",
            description: "Use when you want Lexa to compose a focused context bundle for a specific code task. Best with symbol names, path fragments, or scoped keywords — not free-form natural-language QA. Supports `path_prefix`/`path`, `path_glob`, `language`, and `max_results` (default 10).",
            input_schema: json!({"type":"object","properties":{"task":{"type":"string"},"query":{"type":"string","description":"Alias for task."},"max_results":{"type":"integer"},"max":{"type":"integer","description":"Alias for max_results."},"path_prefix":{"type":"string","description":"Restrict context to a project-relative path prefix."},"path":{"type":"string","description":"Alias for path_prefix."},"path_glob":{"type":"string"},"language":{"type":"string"}},"anyOf":[{"required":["task"]},{"required":["query"]}],"required":[]}),
        },
        ToolSpec {
            name: "trace_deps",
            summary: "Trace import relationships between files.",
            description: "Use to understand import relationships between files. `direction: \"imported_by\"` returns who imports the given file; `direction: \"depends_on\"` returns what it imports (including unresolved local imports separately). Set `transitive: true` to expand the full graph in that direction. External packages are not returned as dependencies.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"direction":{"type":"string","enum":["imported_by","depends_on"]},"transitive":{"type":"boolean"}},"required":["path"]}),
        },
        ToolSpec {
            name: "read",
            summary: "Read file contents, optionally by line range.",
            description: "Use to read file contents with optional line range, compact (trimmed) mode, and `if_hash` to detect changes without re-reading content. Returns the file hash plus content; passing the current hash back returns an `unchanged:<hash>` short response.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"line_start":{"type":"integer"},"line_end":{"type":"integer"},"compact":{"type":"boolean"},"if_hash":{"type":"string"}},"required":["path"]}),
        },
        ToolSpec {
            name: "patch",
            summary: "Apply line-based edits safely with hash checks.",
            description: "Use to apply line-based `replace`, `insert`, or `delete` edits, exact `replace_text`, or anchor-based insertions safely. Always pair with `if_hash` (use `read` first to get the current hash) to prevent stale edits, and run with `dry_run: true` first to preview. Returns the new hash and `change_sequence` after a successful apply.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"op":{"type":"string","enum":["replace","insert","delete"]},"content":{"type":"string"},"range_start":{"type":"integer"},"range_end":{"type":"integer"},"after":{"type":"integer"},"replace_text":{"type":"string"},"anchor":{"type":"string"},"placement":{"type":"string","enum":["before","after"]},"preview_mode":{"type":"string","enum":["compact","full"]},"if_hash":{"type":"string"},"dry_run":{"type":"boolean"}},"required":["path"]}),
        },
        ToolSpec {
            name: "create",
            summary: "Create a new file safely.",
            description: "Use to create a new file safely. Refuses to overwrite an existing file unless `overwrite: true` is set; supports `dry_run` for previewing. On success the file is indexed and a hash plus `change_sequence` are returned.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"overwrite":{"type":"boolean"},"dry_run":{"type":"boolean"}},"required":["path"]}),
        },
        ToolSpec {
            name: "changes",
            summary: "List files changed since a sequence number.",
            description: "Use to see which files have been modified since a given sequence number in the current session. Returns the changed paths with their sequence numbers and operations (replace/insert/delete). Note: change history is session-local and is not persisted across restarts.",
            input_schema: json!({"type":"object","properties":{"since":{"type":"integer"}},"required":[]}),
        },
        ToolSpec {
            name: "recent",
            summary: "List most-recently modified files.",
            description: "Use to find files that were most recently modified, ordered by mtime. Returns path, language, line count, and symbol count. Default limit 10; helpful as a quick \"what just changed\" check.",
            input_schema: json!({"type":"object","properties":{"limit":{"type":"integer"}},"required":[]}),
        },
        ToolSpec {
            name: "status",
            summary: "Show current index statistics.",
            description: "Use to check the current state of the index: file count, symbol count, unique word count, current sequence number, and graph file path/size. Useful before and after `reindex` or `clear_index`.",
            input_schema: json!({"type":"object","properties":{},"required":[]}),
        },
        ToolSpec {
            name: "reindex",
            summary: "Rebuild the in-memory index from scratch.",
            description: "Use to rebuild the in-memory index from scratch after major project changes or when the graph feels stale. Returns the new file/symbol/word counts and persists the graph when persistence is enabled.",
            input_schema: json!({"type":"object","properties":{},"required":[]}),
        },
        ToolSpec {
            name: "clear_index",
            summary: "Drop the in-memory index and graph file.",
            description: "Use to drop the in-memory index and delete the persisted `.lexa/graph.lexa` file (if present). Useful when switching contexts or recovering from a corrupted graph; you will need to reindex afterward.",
            input_schema: json!({"type":"object","properties":{},"required":[]}),
        },
        ToolSpec {
            name: "audit",
            summary: "Run a static, review-oriented architecture audit.",
            description: "Use to run a static, review-oriented architecture audit over the indexed project. Reports import cycles, large files, large symbols, dependency hotspots, and (with `include: [\"dead-code\"]`) unused-code candidates. Not a compiler, typechecker, or linter — a clean audit does not mean the project compiles. Supports `config` (TOML path), `since` (git ref), and `max_results`/`max`.",
            input_schema: json!({"type":"object","properties":{"max_results":{"type":"integer"},"max":{"type":"integer"},"since":{"type":"string"},"config":{"type":"string","description":"Path to a Lexa audit TOML config file, such as lexa.toml or .lexa/audit.toml. This is not a named preset."},"no_config":{"type":"boolean"},"include":{"type":"array","items":{"type":"string","enum":["dead-code"]}}},"required":[]}),
        },
        ToolSpec {
            name: "pipeline",
            summary: "Chain multiple Lexa operations into one query.",
            description: "Use to chain multiple Lexa operations into one composable query instead of calling each tool separately. Prefer the `steps` array form (e.g. `[\"glob src/**/*.rs\", \"search main\", \"limit 5\"]`); each step is one of: `glob`/`find`, `fuzzy`/`path_search`, `search`/`text_search`, `filter`, `outline`, `deps`, `read`, `sort`, `limit`, `count`.",
            input_schema: json!({"type":"object","properties":{"pipeline":{"type":"string","description":"Advanced pipe string, e.g. glob src/**/*.rs | search main | limit 5."},"steps":{"type":"array","items":{"type":"string"},"description":"Recommended form; each item is one pipeline step, e.g. [\"glob src/**/*.rs\", \"search main\", \"limit 5\"]. Put search terms inside the relevant step."}},"anyOf":[{"required":["pipeline"]},{"required":["steps"]}],"required":[]}),
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_count_matches_documented_tool_count() {
        assert_eq!(TOOL_SPECS.len(), 22, "expected 22 tool specs");
    }

    #[test]
    fn specs_have_unique_names() {
        let mut names = TOOL_SPECS.iter().map(|spec| spec.name).collect::<Vec<_>>();
        names.sort_unstable();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "duplicate tool names");
    }

    #[test]
    fn specs_have_non_empty_strings() {
        for spec in TOOL_SPECS.iter() {
            assert!(!spec.name.is_empty(), "empty name");
            assert!(!spec.summary.is_empty(), "{}: empty summary", spec.name);
            assert!(
                !spec.description.is_empty(),
                "{}: empty description",
                spec.name
            );
        }
    }

    #[test]
    fn input_schemas_are_well_formed_objects() {
        for spec in TOOL_SPECS.iter() {
            let obj = spec
                .input_schema
                .as_object()
                .unwrap_or_else(|| panic!("{}: schema is not an object", spec.name));
            assert_eq!(
                obj.get("type").and_then(Value::as_str),
                Some("object"),
                "{}: schema.type must be \"object\"",
                spec.name,
            );
            let props = obj
                .get("properties")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{}: missing properties object", spec.name));
            if let Some(required) = obj.get("required").and_then(Value::as_array) {
                for req in required {
                    let key = req
                        .as_str()
                        .unwrap_or_else(|| panic!("{}: required entry is not a string", spec.name));
                    assert!(
                        props.contains_key(key),
                        "{}: required key '{}' missing from properties",
                        spec.name,
                        key,
                    );
                }
            }
            if let Some(any_of) = obj.get("anyOf").and_then(Value::as_array) {
                for variant in any_of {
                    let required = variant
                        .get("required")
                        .and_then(Value::as_array)
                        .unwrap_or_else(|| {
                            panic!("{}: anyOf variant missing required array", spec.name)
                        });
                    for req in required {
                        let key = req.as_str().unwrap_or_else(|| {
                            panic!("{}: anyOf required entry is not a string", spec.name)
                        });
                        assert!(
                            props.contains_key(key),
                            "{}: anyOf required key '{}' missing from properties",
                            spec.name,
                            key,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn summaries_are_short_for_mcp_tool_list() {
        for spec in TOOL_SPECS.iter() {
            assert!(
                spec.summary.len() <= 120,
                "{}: summary is {} chars",
                spec.name,
                spec.summary.len(),
            );
        }
    }

    #[test]
    fn required_tool_names_are_present() {
        let names = TOOL_SPECS.iter().map(|spec| spec.name).collect::<Vec<_>>();
        for required in [
            "files",
            "list",
            "glob",
            "path_search",
            "outline",
            "symbol_defs",
            "symbol_search",
            "word_refs",
            "text_search",
            "callers",
            "brief",
            "trace_deps",
            "read",
            "patch",
            "create",
            "changes",
            "recent",
            "status",
            "reindex",
            "clear_index",
            "audit",
            "pipeline",
        ] {
            assert!(
                names.contains(&required),
                "missing required tool: {required}"
            );
        }
    }
}
