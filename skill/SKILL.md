---
name: lexa
description: Use Lexa for fast local codebase intelligence. Trigger when a user wants to explore, search, map, read, patch, pipeline, or maintain a project with Lexa's CLI or MCP server; when they ask for symbol lookup, dependency tracing, file outlines, safe line-based edits, or agent-friendly code context.
---

# Lexa

Use Lexa when working inside a codebase that benefits from fast local indexing, symbol lookup, text search, dependency tracing, hash-aware reads, or safe line-based patches.

## Check Availability

First check whether `lexa` is installed:

```bash
lexa --version
```

If unavailable and the Lexa repository is present, install it:

```bash
cargo install --path /path/to/lexa --force
```

If unavailable and the repository path is unknown, ask the user for the Lexa repository or binary path.

To upgrade the installed Lexa binary, use:

```bash
lexa upgrade
```

By default, `upgrade` installs into the directory containing the running `lexa`
binary. Use `lexa upgrade --install-dir <dir>` or `LEXA_INSTALL_DIR` for an
explicit target. Use `lexa index .` to refresh a project's graph. Do not describe
`upgrade` as updating the project index.

## Index The Project

From the target project root:

```bash
lexa index .
```

Lexa writes `.lexa/graph.lexa` by default. Use a custom graph path when the user wants the index outside the project:

```bash
lexa --graph /tmp/project.graph.lexa index .
```

Use `--no-graph` only for temporary in-memory sessions.

## Exploration Workflow

Start broad, then narrow:

```bash
lexa status
lexa files
lexa path-search "<partial-path>"
lexa outline <path>
lexa text-search "<query>" --scope
lexa symbol-defs <name>
lexa trace-deps <path>
lexa audit
```

Prefer Lexa over repeated filesystem scans when the question is about indexed files, symbols, imports, or recurring text search. Use `rg` directly when the user needs raw repository text search, unindexed generated files, or exact grep-style behavior.

## CLI Command Reference

All CLI commands:

| Command | Use |
| --- | --- |
| `index <path>` | Index a project and write the graph |
| `files [path]` | Show indexed files with language, lines, and symbols |
| `list [path]` | List immediate children of an indexed directory |
| `path-search <pattern>` | Fuzzy path search |
| `text-search <query>` | Search indexed text |
| `outline <path>` | Show imports and symbols for one file |
| `symbol-defs <name>` | Find exact symbol definitions |
| `word-refs <word>` | Find exact word or identifier references |
| `trace-deps <path>` | Trace import dependencies |
| `recent` | Show recently modified files |
| `callers <name>` | Find non-definition call sites |
| `brief <task>` | Compose task-focused context |
| `changes [since]` | Show changed files since a sequence number |
| `read <path>` | Read a file, line range, or hash |
| `patch <path> <op>` | Apply `replace`, `insert`, or `delete` edits |
| `create <path>` | Create a file safely |
| `glob <pattern>` | Match indexed paths with a glob |
| `status` | Show index status |
| `audit` | Run a review-oriented architecture audit |
| `upgrade [version]` | Upgrade the Lexa binary, not a project index |
| `watch [path]` | Watch files and refresh the graph |
| `pipeline <pipeline>` | Run composable query stages |
| `mcp [path]` | Start the MCP server over stdio; returns text-only tool content by default |

Important search flags:

```bash
lexa text-search "<query>" --max 20
lexa text-search "<query>" --regex
lexa text-search "<query>" --scope
lexa text-search "<query>" --compact
lexa text-search "<query>" --paths-only
lexa text-search "<query>" --path-glob "**/*.{ts,tsx}"
```

## Pipelines

Use `pipeline` to chain simple query operations:

```bash
lexa pipeline 'glob src/**/*.rs | search Engine | limit 10'
lexa pipeline 'fuzzy parser | outline'
lexa pipeline 'glob src/**/*.rs | deps'
lexa pipeline 'glob src/**/*.rs | count'
```

Pipeline stages:

| Stage | Use |
| --- | --- |
| `find <glob>` / `glob <glob>` | Start from glob-matched files |
| `fuzzy <query>` / `find_path <query>` | Start from fuzzy path matches |
| `search <query>` | Search all files or current file set |
| `filter <text>` | Filter current files/results by text |
| `outline` | Render outlines for current files |
| `deps` | Render dependencies for current files |
| `read` | Render contents for current files |
| `sort` | Sort current files/results |
| `limit [n]` | Truncate current files/results, default `10` |
| `count` | Count current files/results |

## Reading Files

Read focused line ranges instead of entire large files:

```bash
lexa read <path> -L 20-80
```

Use hashes before edits or when avoiding stale reads:

```bash
lexa read <path> --hash
lexa read <path> --if-hash <hash>
```

If Lexa returns `unchanged:<hash>`, do not reread the file unless new context is needed.

## Editing Files

For line-based edits, prefer Lexa patch operations when they match the task:

```bash
lexa patch <path> replace -L 12-14 --content '<new content>'
lexa patch <path> insert --after 20 --content '<new content>'
lexa patch <path> delete -L 40-45
lexa create <path> --content '<new file content>'
```

For safety, pair edits with `--if-hash` when another process or user may have changed the file:

```bash
lexa patch <path> replace -L 12 --if-hash <hash> --content '<new content>'
```

Use the native editor or normal patch tools instead of Lexa when edits are structural, span many non-contiguous ranges, or require formatter-aware rewrites.

## MCP Server

Start Lexa over stdio for agent integrations:

```bash
lexa mcp /path/to/project
```

Use in-memory mode for disposable sessions:

```bash
lexa --no-graph mcp /path/to/project
```

Generic MCP config:

```json
{
  "mcpServers": {
    "lexa": {
      "command": "lexa",
      "args": ["mcp", "/path/to/project"]
    }
  }
}
```

MCP tools exposed by Lexa:

By default MCP tool calls omit duplicated `structuredContent` to reduce model token use. Use `lexa mcp . --structured-content` or `lexa mcp . --json` only when the MCP client needs JSON structured tool results.

| Tool | Use |
| --- | --- |
| `files` | Use at the start of exploration to get an overview of the indexed project. Returns every indexed file with language, line count, byte size, symbol count, and modified time; supports filtering by path prefix, glob, language, and line-count range. Prefer this over `glob` or `path_search` when you want a broad view rather than a targeted lookup. |
| `list` | Use when you need to see the immediate children of one directory, similar to `ls`. Returns files with their metadata (language, line count, symbols) and subdirectories as plain entries. Faster than `files` for inspecting a single folder. |
| `glob` | Use when you have an exact glob pattern (e.g. `src/**/*.rs`) and want matching indexed paths. Returns up to 200 paths with match count and truncation flag. Prefer over `path_search` when the pattern is precise rather than approximate. |
| `path_search` | Use when you only know part of a file name and want fuzzy matches. Returns scored file-path matches ordered by relevance with a configurable limit. Use `query` (or aliases `path`/`pattern`/`name`) and `max_results`/`max` (default 20). |
| `outline` | Use before reading a file to understand its structure. Returns the file's language, line count, imports, and full symbol list (kind, name, line range, detail). Also surfaces unresolved local imports to flag broken references. |
| `symbol_defs` | Use when you know the exact name of a function, class, type, or variable and want its precise definition. Returns every matching definition with file path, line range, kind, and detail string. Use `name` (or alias `query`) as the exact match key. |
| `symbol_search` | Use when you only know part of a symbol name and want fuzzy matches across the project (e.g. `createAgent` matching `createProjectAgent`). Returns scored symbol matches with file, line range, kind, and detail; default limit 20. |
| `word_refs` | Use when you want every occurrence of an exact identifier or word, including definitions and declarations. Acts like `grep -w` over the indexed word index. Use `word` (or alias `query`) as the exact token. |
| `text_search` | Use as the grep equivalent over indexed text. Supports substring or regex queries with scope (show enclosing symbol), compact (trimmed output), paths-only (`path:line` pairs), and `path_glob` filters. Default limit 20; results include file, line number, and matched text. |
| `callers` | Use to find non-definition call sites and usages of a symbol before refactoring. Returns up to 30 results excluding declarations and type aliases, so the list reflects real call impact. Use `name` (or alias `query`) for the exact symbol. |
| `brief` | Use when you want Lexa to compose a focused context bundle for a specific code task. Best with symbol names, path fragments, or scoped keywords — not free-form natural-language QA. Supports `path_prefix`/`path`, `path_glob`, `language`, and `max_results` (default 10). |
| `trace_deps` | Use to understand import relationships between files. `direction: "imported_by"` returns who imports the given file; `direction: "depends_on"` returns what it imports (including unresolved local imports separately). Set `transitive: true` to expand the full graph in that direction. External packages are not returned as dependencies. |
| `read` | Use to read file contents with optional line range, compact (trimmed) mode, and `if_hash` to detect changes without re-reading content. Returns the file hash plus content; passing the current hash back returns an `unchanged:<hash>` short response. |
| `patch` | Use to apply line-based `replace`, `insert`, or `delete` edits safely. Always pair with `if_hash` (use `read` first to get the current hash) to prevent stale edits, and run with `dry_run: true` first to preview. Returns the new hash and `change_sequence` after a successful apply. |
| `create` | Use to create a new file safely. Refuses to overwrite an existing file unless `overwrite: true` is set; supports `dry_run` for previewing. On success the file is indexed and a hash plus `change_sequence` are returned. |
| `changes` | Use to see which files have been modified since a given sequence number in the current session. Returns the changed paths with their sequence numbers and operations (replace/insert/delete). Note: change history is session-local and is not persisted across restarts. |
| `recent` | Use to find files that were most recently modified, ordered by mtime. Returns path, language, line count, byte size, symbol count, and modified time. Default limit 10; helpful as a quick "what just changed" check. |
| `status` | Use to check the current state of the index: file count, symbol count, unique word count, current sequence number, and graph file path/size. Useful before and after `reindex` or `clear_index`. |
| `reindex` | Use to rebuild the in-memory index from scratch after major project changes or when the graph feels stale. Returns the new file/symbol/word counts and persists the graph when persistence is enabled. |
| `clear_index` | Use to drop the in-memory index and delete the persisted `.lexa/graph.lexa` file (if present). Useful when switching contexts or recovering from a corrupted graph; you will need to reindex afterward. |
| `audit` | Use to run a static, review-oriented architecture audit over the indexed project. Reports import cycles, large files, large symbols, dependency hotspots, and (with `include: ["dead-code"]`) unused-code candidates. Not a compiler, typechecker, or linter — a clean audit does not mean the project compiles. Supports `config` (TOML path), `since` (git ref), and `max_results`/`max`. |
| `pipeline` | Use to chain multiple Lexa operations into one composable query instead of calling each tool separately. Prefer the `steps` array form (e.g. `["glob src/**/*.rs", "search main", "limit 5"]`); each step is one of: `glob`/`find`, `fuzzy`/`path_search`, `search`/`text_search`, `filter`, `outline`, `deps`, `read`, `sort`, `limit`, `count`. |

## Audit Workflow

Use `lexa audit` when the user wants a static analysis pass, architecture review,
or agent-friendly risk summary. The audit is read-only and reports import cycles,
large files, large symbols, and dependency hotspots from the indexed graph.

Lexa audit is not a compiler, typechecker, linter, test runner, or build
verifier. A clean audit never means the project compiles. Do not use
`audit:high=0`, `verdict: pass`, or "No audit findings" as a completion
criterion for implementation work.

```bash
lexa audit
lexa --json audit
lexa audit --max 50
lexa audit --since main
lexa audit --since main --strict
lexa audit --config lexa.toml
lexa audit --no-config
lexa audit --include dead-code
```

Use `--since <git-ref>` for review scope and `--strict` when the user wants a
CI-style non-zero exit on high-severity structural findings.
Use `--include dead-code` only when the user explicitly wants unused-code
candidates; treat those findings as candidates, not removal instructions.
Audit findings include `actionability` and `next_steps`. Treat `actionable` as a
likely refactor target, `candidate` as verify-before-change, `expected` as normal
shared infrastructure or composition-root coupling, and `risk_note` as edit with
care but do not assume refactoring is needed.
Human-readable audit output is grouped by actionability. Treat `secondary`
findings as supporting context for a stronger finding on the same file, not a
separate recommendation.
For JSON/MCP output, summarize from `groups.actionable`, `groups.candidates`,
`groups.risk_notes`, `groups.expected`, and `groups.secondary` before consulting
the flat `findings` array.
Dead-code candidates are source-symbol focused by default. Lexa suppresses
style/config/data/tooling/test/generated/declaration files so CSS variables,
JSON keys, package scripts, and framework mount selectors do not dominate the
audit.

Audit config is optional. Lexa discovers `lexa.toml` or `.lexa/audit.toml`
unless `--config` or `--no-config` is used. Dotted rule IDs must be quoted in
TOML. Cross-language generated artifacts, build outputs, lockfiles, and
dependency folders are ignored by default; set `audit.ignore.generated = false`
only when the user explicitly wants generated output included. For example:

```toml
[audit.rules]
"file.large" = "off"
"dead_code.candidate" = "warning"

[audit.ignore]
generated = true

[audit.dead_code]
ignore_symbols = ["main", "handler", "setup"]
entrypoint_globs = ["src/main.*", "src/bin/**"]
```

## Verification

After any Lexa `patch` or `create` that changes source code, run the relevant
project checks before claiming the work is complete. Prefer the repository's own
scripts and metadata (`package.json`, `Cargo.toml`, `pyproject.toml`, etc.).
For Rust projects, default to:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

For JavaScript and TypeScript projects, inspect `package.json` and run the local
typecheck, lint, test, or build scripts that represent the project's normal
verification gate. If the right command is unavailable or fails for unrelated
environment reasons, report that explicitly instead of treating Lexa audit as a
substitute.

## Output Discipline

- Cite paths and line ranges from Lexa results when explaining findings.
- Keep searches scoped with `--path-glob`, `--max`, or focused queries when possible.
- Use `--regex` or `regex:true` when a `text-search` query contains regex
  syntax such as alternation, grouping, anchors, or character classes.
- For range-sensitive edits, use patch dry-run first. After each successful
  hash-guarded patch, use the returned hash or reread the file before another
  hash-guarded patch; do not reuse stale `if_hash` values.
- Re-index after substantial file edits if later Lexa queries must reflect the new state.
- Mention when a result comes from the graph and may be stale because the project has not been re-indexed.
