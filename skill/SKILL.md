---
name: lexa
description: Use Lexa for fast local codebase intelligence and agent workflows. Trigger when Codex or another AI agent needs to explore, search, map, inspect, summarize, audit, or safely edit a project with Lexa's CLI or MCP server; when the task needs symbol lookup, dependency tracing, file outlines, token-efficient context bundles, hash-aware reads, line patches, or project graph maintenance.
---

# Lexa

Lexa is a local codebase intelligence layer for agents. Use it as an indexed map of the project: find the right files and symbols, gather focused context, understand dependency risk, and make hash-aware line edits when that is the safest edit path.

Lexa is not a compiler, typechecker, linter, formatter, or test runner. Use it to decide what to inspect or change, then run the project's normal verification before claiming implementation work is complete.

## Operating Rules

- Run Lexa from the target project root unless using `--graph` or `mcp /path/to/project`.
- Use the default structured text output. It is the agent-oriented, token-efficient format. Do not pass removed output flags such as `--json`, `--structured-content`, or `--json-output`.
- Index before relying on graph answers. Run `lexa status`; if missing or stale, run `lexa index .`.
- Re-index after substantial edits when later Lexa queries must reflect new code.
- Scope searches with `--path-prefix`, `--path-glob`, `--language`, `--max`, or focused queries whenever possible.
- Use `lexa trace-deps <path>` to inspect what the target imports. Use `--reverse` (or `-r`) to find files that import the target, and add `--transitive` (or `-t`) only for recursive impact. The CLI does not accept `--direction`; MCP uses `direction: "depends_on"` or `"imported_by"`.
- Treat Lexa output as graph-derived context. If files changed outside Lexa and the graph was not refreshed, mention possible staleness.
- Use `rg` or normal filesystem tools for raw grep behavior, unindexed generated files, ignored files, or one-off exact text checks.
- Follow the host agent's editing rules. When using Lexa edits, read first, use hashes, dry-run range-sensitive patches, then verify.

## Index The Project

Assume `lexa` is already available in the agent environment. If the command is
missing, report that clearly instead of attempting installation or upgrade.

Create or refresh the project graph:

```bash
lexa index .
```

Lexa writes `.lexa/graph.lexa` by default. Use a graph outside the repo when the user wants no project-local index artifact:

```bash
lexa --graph /tmp/project.graph.lexa index .
```

Use `--no-graph` only for disposable in-memory sessions, usually with MCP.

## Agent Workflow

Use this loop for most codebase tasks:

```bash
lexa status
lexa files <path-prefix> --language <language> --max-results 30
lexa list <directory>
lexa brief "<task with symbols, paths, or keywords>" --path-prefix <scope> --max-results 8
```

Then narrow with the tool that matches the uncertainty:

```bash
lexa path-search "<partial path>" --max 10
lexa glob "packages/core/**/*.ts"
lexa symbol-search "<partial symbol>" --max 10
lexa symbol-defs <ExactSymbol>
lexa callers <ExactSymbol> --max 20
lexa word-refs <ExactWord> --path-prefix <scope> --max-results 25
lexa text-search "<literal text>" --scope --compact --path-glob "**/*.ts"
lexa outline <path>
lexa read <path> -L 20-80 --compact --hash
lexa trace-deps <path>
lexa trace-deps <path> --reverse
lexa trace-deps <path> --reverse --transitive
```

Use `brief` early when the task is implementation-oriented, such as "create agent tool", "add repository guidance", or "wire payment provider". A good `brief` query includes at least one symbol, path fragment, module name, or exact domain keyword. Treat its output as ranked candidates, not an exhaustive answer. Follow low-confidence next steps, and verify critical or missing paths with `symbol-defs`, `text-search`, `outline`, or direct reads even when confidence is high.

## Choosing Tools

| Need | Use |
| --- | --- |
| Project map | `status`, `files`, `list`, `recent` |
| Known path pattern | `glob` |
| Approximate path | `path-search` |
| File structure before reading | `outline` |
| Exact symbol definition | `symbol-defs` |
| Approximate symbol name | `symbol-search` |
| Every exact identifier occurrence | `word-refs` |
| Non-definition call sites | `callers` |
| Literal or regex text | `text-search` |
| Task-focused context bundle | `brief` |
| Imports used by a file | `trace-deps <path>` |
| Files affected by editing a file | `trace-deps <path> --reverse`; add `--transitive` for recursive impact |
| Review-oriented structural risk | `audit` |
| Composed query in one call | `pipeline` |
| Safe line read/edit/create | `read`, `patch`, `create` |
| Index maintenance | `index`, `reindex`, `clear-index`, `watch` |

Important search flags:

```bash
lexa text-search "<query>" --max 20
lexa text-search "<query>" --regex
lexa text-search "<query>" --scope
lexa text-search "<query>" --compact
lexa text-search "<query>" --paths-only
lexa text-search "<query>" --path-glob "**/*.{ts,tsx}"
```

Use `--regex` only when the query contains regex syntax such as alternation, grouping, anchors, or character classes.

## CLI Commands

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
| `watch [path]` | Watch files and refresh the graph |
| `pipeline <pipeline>` | Run composable query stages |
| `mcp [path]` | Start the MCP server over stdio; returns text-only tool content by default |

## Task Examples

Find where to implement a feature:

```bash
lexa brief "create agent tool" --path-prefix packages/core --max-results 6
lexa symbol-search createTool --max 10
lexa outline packages/core/src/tool/create-tool.ts
lexa read packages/core/src/tool/create-tool.ts -L 1-120 --compact --hash
```

Understand how a symbol is used:

```bash
lexa symbol-defs Agent
lexa callers createTool --max 20
lexa word-refs Agent --path-prefix packages/core --max-results 25
```

Review a change or PR checkout:

```bash
lexa index .
lexa audit --since main --max 25
lexa recent --limit 10
lexa changes 0
```

Inspect dependency impact before editing:

```bash
lexa outline packages/core/src/agent/agent.ts
lexa trace-deps packages/core/src/agent/agent.ts
lexa trace-deps packages/core/src/agent/agent.ts --reverse
lexa trace-deps packages/core/src/agent/agent.ts --reverse --transitive
lexa pipeline 'glob packages/core/**/*.ts | search Agent | limit 8'
```

Make a small line-based edit safely:

```bash
lexa read src/main.rs -L 20-80 --hash
lexa patch src/main.rs replace -L 42-44 --if-hash <hash> --content-file /tmp/new-block --dry-run
lexa patch src/main.rs replace -L 42-44 --if-hash <hash> --content-file /tmp/new-block
lexa changes <previous-change-sequence>
lexa index .
```

Prefer `--content-file` for Markdown, code fences, large blocks, shell metacharacters, or multiline content. Use the native editor, repository patch tools, or formatter-aware transforms for broad structural edits, many non-contiguous edits, or generated rewrites.

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

## Read And Edit Details

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

After each successful hash-guarded patch, use the returned hash or reread the file before another hash-guarded patch. Do not reuse stale `if_hash` values.

## MCP Operation

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

Default MCP tool calls return one text content block and omit duplicated `structuredContent` to save tokens. Use that structured text as the agent-facing contract; do not ask Lexa for alternate output modes.

Example MCP tool arguments:

| Tool | Arguments |
| --- | --- |
| `brief` | `{"task":"create agent tool","path_prefix":"packages/core","max_results":6}` |
| `word_refs` | `{"word":"Agent","path_prefix":"packages/core","max_results":25}` |
| `read` | `{"path":"packages/core/src/agent/agent.ts","line_start":1,"line_end":80,"compact":true}` |
| `patch` | `{"path":"packages/core/src/agent/agent.ts","op":"replace","range_start":42,"range_end":44,"if_hash":"<hash>","content":"<new content>","dry_run":true}` |
| `trace_deps` | `{"path":"packages/core/src/agent/agent.ts","direction":"imported_by","transitive":true}` |
| `audit` | `{"since":"main","max_results":25}` |

MCP tools exposed by Lexa:

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

## Audit Operation

Use `lexa audit` when the user wants a static analysis pass, architecture review,
or agent-friendly risk summary. The audit is read-only and reports import cycles,
large files, large symbols, and dependency hotspots from the indexed graph.

Lexa audit is not a compiler, typechecker, linter, test runner, or build
verifier. A clean audit never means the project compiles. Do not use
`audit:high=0`, `verdict: pass`, or "No audit findings" as a completion
criterion for implementation work.

```bash
lexa audit
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
prioritized review candidate, `candidate` as verify-before-change, `expected` as normal
shared infrastructure or composition-root coupling, and `risk_note` as edit with
care but do not assume refactoring is needed. Inspect the reported code and run
the relevant compiler, linter, and tests before changing or removing anything.

Human-readable audit output is grouped by actionability. Treat `secondary`
findings as supporting context for a stronger finding on the same file, not a
separate recommendation.

When audit output exposes groups, summarize from `groups.actionable`, `groups.candidates`,
`groups.risk_notes`, `groups.expected`, and `groups.secondary` before consulting
the flat `findings` array.

Dead-code candidates are source-symbol focused by default. Lexa suppresses
style/config/data/tooling/test/generated/declaration files so CSS variables,
data-file keys, package scripts, and framework mount selectors do not dominate the
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

After any Lexa `patch` or `create` that changes source code, run the relevant project checks before claiming the work is complete. For Lexa's own repository, use:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
```

## Output Discipline

- Cite paths and line ranges from Lexa results when explaining findings.
- Keep searches scoped with `--path-glob`, `--max`, or focused queries when possible.
- Report whether context came from `brief`, `audit`, symbol tools, direct reads, or raw search when that distinction matters.
- Do not treat `brief` confidence as proof of correctness or absence. Verify critical paths with focused searches or direct reads.
- Treat `trace-deps` as graph-derived evidence; verify critical relationships against imports or direct reads.
- Treat every `audit` finding as a review candidate until verified, and never treat a clean audit as a passing build or test result.
- Re-index after substantial file edits if later Lexa queries must reflect the new state.
- Mention when a result comes from the graph and may be stale because the project has not been re-indexed.
