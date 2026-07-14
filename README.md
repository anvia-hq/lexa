# Lexa

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)
[![Status](https://img.shields.io/badge/status-ready%20to%20use-brightgreen.svg)](#development)

![Lexa open source banner](docs/assets/lexa-open-source.png)

Fast local code intelligence for AI agents and humans.

Lexa turns a codebase into a portable, queryable graph for search, context,
dependency tracing, and hash-aware edits. Index once, query from the CLI,
your editor, or an MCP client.

Default CLI and MCP output is structured text, optimized for agent context
instead of duplicated machine-readable envelopes. Treat that text shape as the
stable agent-facing contract.

## Install

With npm (the package is `lexa-index`, while the command remains `lexa`):

```bash
npm install -g lexa-index
lexa --version
```

Or run the precompiled executable without a global installation:

```bash
npx lexa-index --version
```

npm installs the matching precompiled Rust executable; it does not compile
Rust or download a binary during installation. Supported npm platforms are
macOS ARM64/x64 and Linux x64. Windows remains supported through the existing
PowerShell installer below.

The existing shell installers remain supported:

```bash
curl -fsSL https://raw.githubusercontent.com/anvia-hq/lexa/main/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/anvia-hq/lexa/main/install.ps1 | iex
```

Install and upgrade details: [docs/install.md](docs/install.md)

## Quick Start

```bash
lexa index .
lexa status
lexa brief "add request timeout handling" --path-prefix src --max-results 8
lexa symbol-search "createAgent" --max 10
lexa outline src/main.rs
lexa read src/main.rs -L 1-80 --compact --hash
lexa audit --max 25
lexa mcp .
```

## Agent Workflow

Use Lexa as the project map, not as the final verifier:

1. Build or refresh the graph with `lexa index .`.
2. Start with `lexa brief "<task>" --path-prefix <scope>` for task-focused context.
3. Narrow with `symbol-search`, `symbol-defs`, `word-refs`, `callers`, `text-search`, and `outline`.
4. Read small ranges with `lexa read <path> -L <start>-<end> --compact --hash`.
5. Inspect impact with `trace-deps`, `pipeline`, and `audit`.
6. For small line edits, use `patch --if-hash` and dry-run first.
7. Run the project's normal tests, typechecks, linters, or build before calling the work done.

Common agent commands:

| Need | Command |
| --- | --- |
| Project overview | `lexa files`, `lexa list <dir>`, `lexa recent` |
| Task context | `lexa brief "<task>" --path-prefix <scope> --max-results 8` |
| Symbols | `lexa symbol-search <query>`, `lexa symbol-defs <ExactName>` |
| References and calls | `lexa word-refs <word>`, `lexa callers <name>` |
| Source text | `lexa text-search "<query>" --scope --compact` |
| File structure | `lexa outline <path>` |
| Dependency impact | `lexa trace-deps <path>` |
| Review signal | `lexa audit --max 25` |
| Safe reads/edits | `lexa read --hash`, `lexa patch --if-hash --dry-run` |

## MCP For Agents

```bash
lexa mcp /path/to/project
```

MCP returns text-only tool content by default to avoid duplicating every result
across multiple output shapes. Lexa does not require agents to choose between
formats; use the structured text returned by the tool call.

## Agent Benchmark

Lexa includes deterministic agent tool-suite benchmarks over a synthetic
project with known ground truth. Token counts are estimated from default
structured-text CLI output and default MCP text content; decoded structured
values are used internally for stable correctness checks.

Run the benchmark suite:

```bash
cargo test --test agent_retrieval_benchmark -- --nocapture
cargo test --test agent_cli_output_format_benchmark -- --nocapture
cargo test --test agent_edit_safety_benchmark -- --nocapture
cargo test --test agent_mcp_session_benchmark -- --nocapture
cargo test --test agent_maintenance_benchmark -- --nocapture
```

Latest local result, June 30, 2026:

| Suite | Checks | Lexa est. tokens | Baseline est. tokens | Aggregate reduction | Notes |
| --- | ---: | ---: | ---: | ---: | --- |
| Retrieval | 13/13 | 1678 | 8397 | 80.0% | Indexed answers against listing, grep, and candidate reads. |
| CLI output format | 13 cases | 1676 | n/a | n/a | Current structured-text CLI envelope across retrieval commands. |
| Edit safety | 11/11 | 709 | n/a | n/a | Hash-aware reads, dry runs, patches, creates, and rejection paths. |
| MCP session | 5/5 | 281 | n/a | n/a | Persistent MCP session edits and state queries. |
| Maintenance | 5/5 | 611 | n/a | n/a | Recent files, status, reindex, audit, and clear-index. |

Correctness checks: **34/34 passed**. The CLI output format suite separately
measures 13 command outputs and validates that each decodes as the expected
structured-text tool result.

Detailed retrieval result:

Retrieval baselines model the non-indexed workflow an agent would usually use:
recursive file listing, grep-style text search, and reading candidate files when
grep alone cannot answer the question. Safety and state suites do not have a
meaningful aggregate grep/read baseline, so they report correctness and token
envelope only.

| Suite | Task | Tool | Compared against | Lexa est. tokens | Baseline est. tokens | Reduction | Correct |
| --- | --- | --- | --- | ---: | ---: | ---: | --- |
| Retrieval | filtered file overview | `files` | recursive file listing filtered to `src/**/*.rs` | 85 | 32 | -165.6% | true |
| Retrieval | directory children | `list` | recursive file listing under `src` | 102 | 52 | -96.2% | true |
| Retrieval | glob paths | `glob` | file listing filtered to `src/*.ts` | 30 | 20 | -50.0% | true |
| Retrieval | fuzzy path | `path_search` | full file listing for agent-side fuzzy matching | 28 | 129 | 78.3% | true |
| Retrieval | scoped text search | `text_search` | scoped grep plus candidate file read | 177 | 105 | -68.6% | true |
| Retrieval | exact word refs | `word_refs` | grep exact word across project | 304 | 274 | -10.9% | true |
| Retrieval | exact definition | `symbol_defs` | grep symbol name plus candidate file reads | 48 | 1768 | 97.3% | true |
| Retrieval | fuzzy symbol | `symbol_search` | grep query terms plus candidate file reads | 86 | 1462 | 94.1% | true |
| Retrieval | callers | `callers` | grep symbol name plus candidate file reads | 330 | 945 | 65.1% | true |
| Retrieval | outline | `outline` | full source file read | 104 | 107 | 2.8% | true |
| Retrieval | dependencies | `trace_deps` | grep imports/requires plus candidate file read | 32 | 113 | 71.7% | true |
| Retrieval | brief | `brief` | grep query terms plus candidate file reads | 220 | 2445 | 91.0% | true |
| Retrieval | composed query | `pipeline` | grep symbol name plus candidate file reads | 132 | 945 | 86.0% | true |

## Website

Project site: **[lexa.anvia.dev](https://lexa.anvia.dev)**

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --locked
```

## License

MIT
