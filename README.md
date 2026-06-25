# Lexa

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)
[![Status](https://img.shields.io/badge/status-ready%20to%20use-brightgreen.svg)](#development)

![Lexa open source banner](docs/assets/lexa-open-source.png)

Fast local code intelligence for humans and AI agents.

Lexa turns a codebase into a portable, queryable graph for search, context,
dependency tracing, and hash-aware edits. Index once, query from the CLI,
your editor, or an MCP client.

## Install

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
lexa text-search "handle_request" --scope
lexa symbol-search "createAgent"
lexa outline src/main.rs
lexa audit
lexa mcp .
```

## Agent Benchmark

Lexa includes deterministic agent tool-suite benchmarks over a synthetic
project with known ground truth. Token counts are estimated from default
non-JSON CLI output and default MCP text content; JSON is used only internally
for stable correctness checks.

Run the retrieval benchmark:

```bash
cargo test --test agent_retrieval_benchmark -- --nocapture
```

Latest local result:

| Suite | Accuracy | Lexa est. tokens | Baseline est. tokens | Aggregate reduction |
| --- | ---: | ---: | ---: | ---: |
| Retrieval | 13/13 | 1914 | 8397 | 77.2% |

Retrieval correctness: **13/13 tasks passed**.

Detailed result:

Retrieval baselines model the non-indexed workflow an agent would usually use:
recursive file listing, grep-style text search, and reading candidate files when
grep alone cannot answer the question.

| Suite | Task | Tool | Compared against | Lexa est. tokens | Baseline est. tokens | Reduction | Correct |
| --- | --- | --- | --- | ---: | ---: | ---: | --- |
| Retrieval | filtered file overview | `files` | recursive file listing filtered to `src/**/*.rs` | 174 | 32 | -443.8% | true |
| Retrieval | directory children | `list` | recursive file listing under `src` | 307 | 52 | -490.4% | true |
| Retrieval | glob paths | `glob` | file listing filtered to `src/*.ts` | 29 | 20 | -45.0% | true |
| Retrieval | fuzzy path | `path_search` | full file listing for agent-side fuzzy matching | 17 | 129 | 86.8% | true |
| Retrieval | scoped text search | `text_search` | scoped grep plus candidate file read | 149 | 105 | -41.9% | true |
| Retrieval | exact word refs | `word_refs` | grep exact word across project | 233 | 274 | 15.0% | true |
| Retrieval | exact definition | `symbol_defs` | grep symbol name plus candidate file reads | 25 | 1768 | 98.6% | true |
| Retrieval | fuzzy symbol | `symbol_search` | grep query terms plus candidate file reads | 56 | 1462 | 96.2% | true |
| Retrieval | callers | `callers` | grep symbol name plus candidate file reads | 296 | 945 | 68.7% | true |
| Retrieval | outline | `outline` | full source file read | 80 | 107 | 25.2% | true |
| Retrieval | dependencies | `trace_deps` | grep imports/requires plus candidate file read | 19 | 113 | 83.2% | true |
| Retrieval | brief | `brief` | grep query terms plus candidate file reads | 441 | 2445 | 82.0% | true |
| Retrieval | composed query | `pipeline` | grep symbol name plus candidate file reads | 88 | 945 | 90.7% | true |

## Docs

Full documentation: **[lexa.anvia.dev](https://lexa.anvia.dev)**

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## License

MIT
