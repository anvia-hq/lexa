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
| `mcp [path]` | Start the MCP server over stdio |

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

| Tool | Use |
| --- | --- |
| `files` | Whole-repo file map |
| `list` | Directory listing |
| `glob` | Glob path matching |
| `path_search` | Fuzzy file path search |
| `outline` | File symbols and imports |
| `symbol_defs` | Exact symbol definitions |
| `word_refs` | Exact word or identifier references |
| `text_search` | Text search with regex/scope/compact/path filters |
| `callers` | Non-definition call sites |
| `brief` | Task-focused context |
| `trace_deps` | Import dependency tracing |
| `read` | Hash-aware file reads |
| `patch` | Atomic line edits |
| `create` | Safe file creation |
| `changes` | Changed files since sequence |
| `recent` | Recently modified files |
| `status` | Index status |
| `audit` | Review-oriented architecture audit |
| `pipeline` | Composable query pipeline |

## Audit Workflow

Use `lexa audit` when the user wants a static analysis pass, architecture review,
or agent-friendly risk summary. The audit is read-only and reports import cycles,
large files, large symbols, and dependency hotspots from the indexed graph.

```bash
lexa audit
lexa --json audit
lexa audit --max 50
lexa audit --since main
lexa audit --since main --strict
```

Use `--since <git-ref>` for review scope and `--strict` when the user wants a
CI-style non-zero exit on high-severity findings.

## Verification

After indexing or editing, run the relevant project checks. For Rust projects, default to:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

For other stacks, inspect the project scripts or package metadata and choose the local lint/test commands already used by the repository.

## Output Discipline

- Cite paths and line ranges from Lexa results when explaining findings.
- Keep searches scoped with `--path-glob`, `--max`, or focused queries when possible.
- Re-index after substantial file edits if later Lexa queries must reflect the new state.
- Mention when a result comes from the graph and may be stale because the project has not been re-indexed.
