<!-- TOOLS START -->
# MCP Tools Reference

> Generated from `TOOL_SPECS` in `src/mcp/tool_spec.rs`. Do not edit by hand; run `just gen-skill` to regenerate.

## files

**Summary:** Start here for an overview of the indexed project.

**Description:** Use at the start of exploration to get an overview of the indexed project. Returns every indexed file with language, line count, byte size, symbol count, and modified time; supports filtering by path prefix, glob, language, and line-count range. Prefer this over `glob` or `path_search` when you want a broad view rather than a targeted lookup.

**Input schema:**

```json
{
  "properties": {
    "language": {
      "description": "Language name such as typescript, rust, json, or markdown.",
      "type": "string"
    },
    "max": {
      "description": "Alias for max_results.",
      "type": "integer"
    },
    "max_lines": {
      "type": "integer"
    },
    "max_results": {
      "type": "integer"
    },
    "min_lines": {
      "type": "integer"
    },
    "path": {
      "description": "Optional project-relative path prefix.",
      "type": "string"
    },
    "path_glob": {
      "type": "string"
    }
  },
  "required": [],
  "type": "object"
}
```

## list

**Summary:** List immediate children of one directory.

**Description:** Use when you need to see the immediate children of one directory, similar to `ls`. Returns files with their metadata (language, line count, symbols) and subdirectories as plain entries. Faster than `files` for inspecting a single folder.

**Input schema:**

```json
{
  "properties": {
    "path": {
      "type": "string"
    }
  },
  "required": [],
  "type": "object"
}
```

## glob

**Summary:** Match indexed paths with an exact glob pattern.

**Description:** Use when you have an exact glob pattern (e.g. `src/**/*.rs`) and want matching indexed paths. Returns up to 200 paths with match count and truncation flag. Prefer over `path_search` when the pattern is precise rather than approximate.

**Input schema:**

```json
{
  "properties": {
    "pattern": {
      "type": "string"
    }
  },
  "required": [
    "pattern"
  ],
  "type": "object"
}
```

## path_search

**Summary:** Fuzzy-match indexed file paths.

**Description:** Use when you only know part of a file name and want fuzzy matches. Returns scored file-path matches ordered by relevance with a configurable limit. Use `query` (or aliases `path`/`pattern`/`name`) and `max_results`/`max` (default 20).

**Input schema:**

```json
{
  "properties": {
    "max": {
      "type": "integer"
    },
    "max_results": {
      "type": "integer"
    },
    "query": {
      "type": "string"
    }
  },
  "required": [
    "query"
  ],
  "type": "object"
}
```

## outline

**Summary:** Get the imports and symbol list of one file.

**Description:** Use before reading a file to understand its structure. Returns the file's language, line count, imports, and full symbol list (kind, name, line range, detail). Also surfaces unresolved local imports to flag broken references.

**Input schema:**

```json
{
  "properties": {
    "path": {
      "type": "string"
    }
  },
  "required": [
    "path"
  ],
  "type": "object"
}
```

## symbol_defs

**Summary:** Find definitions of an exact symbol name.

**Description:** Use when you know the exact name of a function, class, type, or variable and want its precise definition. Returns every matching definition with file path, line range, kind, and detail string. Use `name` (or alias `query`) as the exact match key.

**Input schema:**

```json
{
  "properties": {
    "name": {
      "type": "string"
    }
  },
  "required": [
    "name"
  ],
  "type": "object"
}
```

## symbol_search

**Summary:** Fuzzy-match symbol names across the project.

**Description:** Use when you only know part of a symbol name and want fuzzy matches across the project (e.g. `createAgent` matching `createProjectAgent`). Returns scored symbol matches with file, line range, kind, and detail; default limit 20.

**Input schema:**

```json
{
  "properties": {
    "max": {
      "description": "Alias for max_results.",
      "type": "integer"
    },
    "max_results": {
      "type": "integer"
    },
    "query": {
      "type": "string"
    }
  },
  "required": [
    "query"
  ],
  "type": "object"
}
```

## word_refs

**Summary:** Find every occurrence of an exact identifier.

**Description:** Use when you want every occurrence of an exact identifier or word, including definitions and declarations. Acts like `grep -w` over the indexed word index. Use `word` (or alias `query`) as the exact token.

**Input schema:**

```json
{
  "properties": {
    "word": {
      "type": "string"
    }
  },
  "required": [
    "word"
  ],
  "type": "object"
}
```

## text_search

**Summary:** Substring or regex search over indexed text.

**Description:** Use as the grep equivalent over indexed text. Supports substring or regex queries with scope (show enclosing symbol), compact (trimmed output), paths-only (`path:line` pairs), and `path_glob` filters. Default limit 20; results include file, line number, and matched text.

**Input schema:**

```json
{
  "properties": {
    "compact": {
      "type": "boolean"
    },
    "max_results": {
      "type": "integer"
    },
    "path_glob": {
      "type": "string"
    },
    "paths_only": {
      "type": "boolean"
    },
    "query": {
      "type": "string"
    },
    "regex": {
      "type": "boolean"
    },
    "scope": {
      "type": "boolean"
    }
  },
  "required": [
    "query"
  ],
  "type": "object"
}
```

## callers

**Summary:** Find non-definition call sites of a symbol.

**Description:** Use to find non-definition call sites and usages of a symbol before refactoring. Returns up to 30 results excluding declarations and type aliases, so the list reflects real call impact. Use `name` (or alias `query`) for the exact symbol.

**Input schema:**

```json
{
  "properties": {
    "name": {
      "type": "string"
    }
  },
  "required": [
    "name"
  ],
  "type": "object"
}
```

## brief

**Summary:** Compose a focused context bundle for a code task.

**Description:** Use when you want Lexa to compose a focused context bundle for a specific code task. Best with symbol names, path fragments, or scoped keywords — not free-form natural-language QA. Supports `path_prefix`/`path`, `path_glob`, `language`, and `max_results` (default 10).

**Input schema:**

```json
{
  "properties": {
    "language": {
      "type": "string"
    },
    "max": {
      "description": "Alias for max_results.",
      "type": "integer"
    },
    "max_results": {
      "type": "integer"
    },
    "path": {
      "description": "Alias for path_prefix.",
      "type": "string"
    },
    "path_glob": {
      "type": "string"
    },
    "path_prefix": {
      "description": "Restrict context to a project-relative path prefix.",
      "type": "string"
    },
    "task": {
      "type": "string"
    }
  },
  "required": [
    "task"
  ],
  "type": "object"
}
```

## trace_deps

**Summary:** Trace import relationships between files.

**Description:** Use to understand import relationships between files. `direction: "imported_by"` returns who imports the given file; `direction: "depends_on"` returns what it imports (including unresolved local imports separately). Set `transitive: true` to expand the full graph in that direction. External packages are not returned as dependencies.

**Input schema:**

```json
{
  "properties": {
    "direction": {
      "enum": [
        "imported_by",
        "depends_on"
      ],
      "type": "string"
    },
    "path": {
      "type": "string"
    },
    "transitive": {
      "type": "boolean"
    }
  },
  "required": [
    "path"
  ],
  "type": "object"
}
```

## read

**Summary:** Read file contents, optionally by line range.

**Description:** Use to read file contents with optional line range, compact (trimmed) mode, and `if_hash` to detect changes without re-reading content. Returns the file hash plus content; passing the current hash back returns an `unchanged:<hash>` short response.

**Input schema:**

```json
{
  "properties": {
    "compact": {
      "type": "boolean"
    },
    "if_hash": {
      "type": "string"
    },
    "line_end": {
      "type": "integer"
    },
    "line_start": {
      "type": "integer"
    },
    "path": {
      "type": "string"
    }
  },
  "required": [
    "path"
  ],
  "type": "object"
}
```

## patch

**Summary:** Apply line-based edits safely with hash checks.

**Description:** Use to apply line-based `replace`, `insert`, or `delete` edits, exact `replace_text`, or anchor-based insertions safely. Always pair with `if_hash` (use `read` first to get the current hash) to prevent stale edits, and run with `dry_run: true` first to preview. Returns the new hash and `change_sequence` after a successful apply.

**Input schema:**

```json
{
  "properties": {
    "after": {
      "type": "integer"
    },
    "anchor": {
      "type": "string"
    },
    "content": {
      "type": "string"
    },
    "dry_run": {
      "type": "boolean"
    },
    "if_hash": {
      "type": "string"
    },
    "op": {
      "enum": [
        "replace",
        "insert",
        "delete"
      ],
      "type": "string"
    },
    "path": {
      "type": "string"
    },
    "placement": {
      "enum": [
        "before",
        "after"
      ],
      "type": "string"
    },
    "preview_mode": {
      "enum": [
        "compact",
        "full"
      ],
      "type": "string"
    },
    "range_end": {
      "type": "integer"
    },
    "range_start": {
      "type": "integer"
    },
    "replace_text": {
      "type": "string"
    }
  },
  "required": [
    "path"
  ],
  "type": "object"
}
```

## create

**Summary:** Create a new file safely.

**Description:** Use to create a new file safely. Refuses to overwrite an existing file unless `overwrite: true` is set; supports `dry_run` for previewing. On success the file is indexed and a hash plus `change_sequence` are returned.

**Input schema:**

```json
{
  "properties": {
    "content": {
      "type": "string"
    },
    "dry_run": {
      "type": "boolean"
    },
    "overwrite": {
      "type": "boolean"
    },
    "path": {
      "type": "string"
    }
  },
  "required": [
    "path"
  ],
  "type": "object"
}
```

## changes

**Summary:** List files changed since a sequence number.

**Description:** Use to see which files have been modified since a given sequence number in the current session. Returns the changed paths with their sequence numbers and operations (replace/insert/delete). Note: change history is session-local and is not persisted across restarts.

**Input schema:**

```json
{
  "properties": {
    "since": {
      "type": "integer"
    }
  },
  "required": [],
  "type": "object"
}
```

## recent

**Summary:** List most-recently modified files.

**Description:** Use to find files that were most recently modified, ordered by mtime. Returns path, language, line count, byte size, symbol count, and modified time. Default limit 10; helpful as a quick "what just changed" check.

**Input schema:**

```json
{
  "properties": {
    "limit": {
      "type": "integer"
    }
  },
  "required": [],
  "type": "object"
}
```

## status

**Summary:** Show current index statistics.

**Description:** Use to check the current state of the index: file count, symbol count, unique word count, current sequence number, and graph file path/size. Useful before and after `reindex` or `clear_index`.

**Input schema:**

```json
{
  "properties": {},
  "required": [],
  "type": "object"
}
```

## reindex

**Summary:** Rebuild the in-memory index from scratch.

**Description:** Use to rebuild the in-memory index from scratch after major project changes or when the graph feels stale. Returns the new file/symbol/word counts and persists the graph when persistence is enabled.

**Input schema:**

```json
{
  "properties": {},
  "required": [],
  "type": "object"
}
```

## clear_index

**Summary:** Drop the in-memory index and graph file.

**Description:** Use to drop the in-memory index and delete the persisted `.lexa/graph.lexa` file (if present). Useful when switching contexts or recovering from a corrupted graph; you will need to reindex afterward.

**Input schema:**

```json
{
  "properties": {},
  "required": [],
  "type": "object"
}
```

## audit

**Summary:** Run a static, review-oriented architecture audit.

**Description:** Use to run a static, review-oriented architecture audit over the indexed project. Reports import cycles, large files, large symbols, dependency hotspots, and (with `include: ["dead-code"]`) unused-code candidates. Not a compiler, typechecker, or linter — a clean audit does not mean the project compiles. Supports `config` (TOML path), `since` (git ref), and `max_results`/`max`.

**Input schema:**

```json
{
  "properties": {
    "config": {
      "description": "Path to a Lexa audit TOML config file, such as lexa.toml or .lexa/audit.toml. This is not a named preset.",
      "type": "string"
    },
    "include": {
      "items": {
        "enum": [
          "dead-code"
        ],
        "type": "string"
      },
      "type": "array"
    },
    "max": {
      "type": "integer"
    },
    "max_results": {
      "type": "integer"
    },
    "no_config": {
      "type": "boolean"
    },
    "since": {
      "type": "string"
    }
  },
  "required": [],
  "type": "object"
}
```

## pipeline

**Summary:** Chain multiple Lexa operations into one query.

**Description:** Use to chain multiple Lexa operations into one composable query instead of calling each tool separately. Prefer the `steps` array form (e.g. `["glob src/**/*.rs", "search main", "limit 5"]`); each step is one of: `glob`/`find`, `fuzzy`/`path_search`, `search`/`text_search`, `filter`, `outline`, `deps`, `read`, `sort`, `limit`, `count`.

**Input schema:**

```json
{
  "properties": {
    "pipeline": {
      "description": "Advanced pipe string, e.g. glob src/**/*.rs | search main | limit 5.",
      "type": "string"
    },
    "steps": {
      "description": "Recommended form; each item is one pipeline step, e.g. [\"glob src/**/*.rs\", \"search main\", \"limit 5\"]. Put search terms inside the relevant step.",
      "items": {
        "type": "string"
      },
      "type": "array"
    }
  },
  "required": [],
  "type": "object"
}
```

<!-- TOOLS END -->
