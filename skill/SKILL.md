---
name: lexa
description: Use Lexa for fast local codebase intelligence. Trigger when a user wants to explore, search, map, read, patch, pipeline, or maintain a project with Lexa's CLI or MCP server; when they ask for symbol lookup, dependency tracing, file outlines, safe line-based edits, or agent-friendly code context.
---

# Lexa

Use Lexa when working inside a codebase that benefits from fast local indexing, symbol lookup, text search, dependency tracing, hash-aware reads, or safe line-based patches.

## Check Availability

Check whether `lexa` is installed:

```bash
lexa --version
```

If unavailable, install it from the repository or follow [docs/install.md](https://github.com/anvia-hq/lexa/blob/main/docs/install.md):

```bash
cargo install --path /path/to/lexa --force
```

Use `lexa upgrade` to upgrade the installed Lexa binary. `upgrade` updates the binary, not the project index; use `lexa index .` to refresh a project's graph.

```bash
lexa upgrade
```

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
lexa read <path> --line-start 20 --line-end 80
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
lexa patch <path> --replace-text '<old exact text>' --content '<new content>'
lexa patch <path> --anchor '<unique exact anchor>' --placement after --content '<new content>'
lexa create <path> --content '<new file content>'
```

For large replacements, Markdown, code fences, or content with shell
metacharacters, write the new content to a temp file and use
`--content-file <path>` instead of inline `--content`.

For safety, pair edits with `--if-hash` when another process or user may have changed the file:

```bash
lexa patch <path> replace -L 12 --if-hash <hash> --content '<new content>'
```

Use `--dry-run --preview compact` before range-sensitive edits when you need a
focused preview. Compact preview is the default for patch dry runs.

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

<!-- TOOLS START -->
| Tool | Use |
| --- | --- |
| `files` | Start here for an overview of the indexed project. |
| `list` | List immediate children of one directory. |
| `glob` | Match indexed paths with an exact glob pattern. |
| `path_search` | Fuzzy-match indexed file paths. |
| `outline` | Get the imports and symbol list of one file. |
| `symbol_defs` | Find definitions of an exact symbol name. |
| `symbol_search` | Fuzzy-match symbol names across the project. |
| `word_refs` | Find every occurrence of an exact identifier. |
| `text_search` | Substring or regex search over indexed text. |
| `callers` | Find non-definition call sites of a symbol. |
| `brief` | Compose a focused context bundle for a code task. |
| `trace_deps` | Trace import relationships between files. |
| `read` | Read file contents, optionally by line range. |
| `patch` | Apply line-based edits safely with hash checks. |
| `create` | Create a new file safely. |
| `changes` | List files changed since a sequence number. |
| `recent` | List most-recently modified files. |
| `status` | Show current index statistics. |
| `reindex` | Rebuild the in-memory index from scratch. |
| `clear_index` | Drop the in-memory index and graph file. |
| `audit` | Run a static, review-oriented architecture audit. |
| `pipeline` | Chain multiple Lexa operations into one query. |

<!-- TOOLS END -->

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

After any Lexa `patch` or `create` that changes source code, run the relevant project checks before claiming the work is complete. For Lexa's own verification commands, see [CONTRIBUTING.md](https://github.com/anvia-hq/lexa/blob/main/CONTRIBUTING.md#development-checks).

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
