# Lexa

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)
[![MCP](https://img.shields.io/badge/MCP-ready-4b5563.svg)](#mcp)
[![Status](https://img.shields.io/badge/status-ready%20to%20use-brightgreen.svg)](#development)

![Lexa open source banner](docs/assets/lexa-open-source.png)

Fast local code intelligence for humans and AI agents.

Lexa turns a codebase into a portable, queryable graph so every tool can work
from the same stable view of the project.

Instead of repeatedly scanning files ad hoc, Lexa indexes structure, text,
symbols, imports, content hashes, and recent edits into one local graph. That
method gives agents compact context, traceable lookups, hash-aware reads, and
atomic line-based patches.

```bash
lexa index .
lexa text-search "handle_request" --scope
lexa outline src/main.rs
lexa audit
lexa mcp .
```

| Project | Info |
| --- | --- |
| Interface | CLI and MCP server |
| Index | `.lexa/graph.lexa` by default |
| Runtime | Native Rust binary |
| License | MIT |

## Why Lexa

Lexa is built around an index-first workflow:

1. Build one local graph for the project.
2. Query that graph for paths, symbols, text, outlines, imports, and context.
3. Read and patch files with content hashes so edits can be checked against the
   version that was inspected.

That makes Lexa useful as a shared context layer between a developer, a terminal
workflow, and an AI agent.

## Install

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/anvia-hq/lexa/main/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/anvia-hq/lexa/main/install.ps1 | iex
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/anvia-hq/lexa/main/install.sh | sh -s -- v0.3.0
```

From source:

```bash
cargo install --path .
```

Upgrade an installed release:

```bash
lexa upgrade
lexa upgrade v0.3.0
lexa upgrade --install-dir "$HOME/.local/bin"
```

`upgrade` updates the Lexa binary in the directory containing the currently
running `lexa`, unless `--install-dir` or `LEXA_INSTALL_DIR` is set. To refresh a
project's graph, run `lexa index .`.

Or build without installing:

```bash
cargo build --release
./target/release/lexa --help
```

Make sure Cargo's bin directory is on your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Quick Start

```bash
lexa index /path/to/project
cd /path/to/project

lexa files
lexa text-search "handle_request"
lexa outline src/main.rs
lexa symbol-defs Engine
lexa read src/main.rs -L 1-80
```

`index` writes the graph to `.lexa/graph.lexa` by default. Commands run from the
project root will read that graph automatically.

Use a custom graph path:

```bash
lexa --graph /tmp/project.graph.lexa index /path/to/project
lexa --graph /tmp/project.graph.lexa text-search "Parser"
```

## Commands

| Command | Purpose |
| --- | --- |
| `index <path>` | Index a project and write a graph |
| `files [path]` | Show indexed files |
| `list [path]` | List directory children |
| `glob <pattern>` | Match indexed paths |
| `path-search <pattern>` | Fuzzy path search |
| `text-search <query>` | Search indexed text |
| `outline <path>` | Show imports and symbols |
| `symbol-defs <name>` | Find exact symbol definitions |
| `word-refs <word>` | Find exact word or identifier references |
| `callers <name>` | Find non-definition call sites |
| `trace-deps <path>` | Trace parsed imports |
| `brief <task>` | Build task-focused context |
| `read <path>` | Read a file or line range |
| `patch <path> <op>` | Apply a line-based edit |
| `create <path>` | Create a file safely |
| `changes [since]` | Show session-local changes |
| `recent` | Show recently modified files |
| `status` | Show index status |
| `audit` | Run a review-oriented architecture audit |
| `upgrade [version]` | Upgrade the Lexa binary, not a project index |
| `watch [path]` | Refresh graph on file changes |
| `pipeline <pipeline>` | Chain query operations |
| `mcp [path]` | Start MCP over stdio |

Useful search flags:

```bash
lexa text-search "render" --scope
lexa text-search --regex "render[A-Z]\\w+"
lexa text-search "useEffect" --path-glob "**/*.{ts,tsx}"
lexa text-search "TODO" --compact --paths-only
```

Safe edit example:

```bash
lexa read src/main.rs --hash
lexa patch src/main.rs replace -L 12 --if-hash <hash> --content '    println!("updated");'
lexa create src/new_file.rs --content 'pub fn new_file() {}'
```

Audit a project for structural review risks:

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

`audit` is read-only. It flags import cycles, large files, large symbols, and
dependency hotspots from the indexed graph. Use `--since` to scope findings to
changed files and their direct dependency context. Use `--strict` to return a
non-zero exit code when high-severity findings are present. Config is optional;
Lexa discovers `lexa.toml` or `.lexa/audit.toml` unless `--config` or
`--no-config` is used. Dead-code candidates are read-only and off by default;
enable them with `--include dead-code` or config. Generated artifacts are
ignored by default across common languages and frameworks, including generated
directories, protobuf/gRPC outputs, Android/Qt/Dart/C#/OpenAPI/GraphQL outputs,
lockfiles, build output, dependency folders, and common tool-specific files like
`worker-configuration.d.ts`, `routeTree.gen.ts`, and Drizzle metadata.

Findings include an `actionability` classification and `next_steps` hints in
JSON/MCP output. `actionable` means the finding is a likely refactor target,
`candidate` means verify before changing, `expected` means the dependency shape
is normal for a shared primitive or composition root, and `risk_note` means edit
carefully but do not assume a refactor is required.
Human-readable audit output is grouped by actionability. When a lower-priority
finding appears on the same file as a stronger actionable finding, it is marked
as `secondary` so agents keep it as context instead of treating it as another
independent action item.
JSON and MCP output include both the compatibility-preserving flat `findings`
array and grouped buckets under `groups`: `primary`, `secondary`, `actionable`,
`candidates`, `risk_notes`, and `expected`. Agents should summarize from
`groups` first and use `findings` only for full-detail traversal.

Dead-code candidates are limited to source-code symbols by default. Lexa skips
style sheets, data/config files, package manifests, common framework config
files, tests, generated artifacts, and declaration files to avoid reporting
tooling keys, CSS tokens, or framework mount selectors as unused code.

Minimal audit config:

```toml
[audit]
max_findings = 100

[audit.thresholds]
large_file_warning = 800
large_file_high = 1500
large_symbol_warning = 120
large_symbol_high = 250
fan_in_warning = 15
fan_in_high = 40
fan_out_warning = 20
fan_out_high = 50

[audit.rules]
"architecture.cycle" = "high"
"file.large" = "warning"
"symbol.large" = "warning"
"dependency.hotspot" = "warning"
"dead_code.candidate" = "off"

[audit.ignore]
generated = true
paths = ["target/**", "vendor/**"]
findings = ["dependency.hotspot:src/main.rs"]

[audit.dead_code]
ignore_symbols = ["main", "handler", "setup"]
entrypoint_globs = ["src/main.*", "src/bin/**", "pages/**", "app/**"]
```

## MCP

Expose the same graph-backed tools to an MCP client:

```bash
lexa index /path/to/project
lexa mcp /path/to/project
```

MCP refreshes the graph before startup and watches the project while running.
External edit events are applied to the same in-memory graph before the next MCP
request is handled, so tools see fresh content without restarting the server.
Run `lexa index` first when you want MCP to start from a fully rebuilt graph
instead of relying on the cheaper startup freshness check.
Disable both the startup refresh and runtime watcher when you want to trust the
existing graph exactly:

```bash
lexa mcp /path/to/project --no-refresh
```

Tune the watcher debounce interval when needed:

```bash
lexa mcp /path/to/project --debounce 250
```

Run MCP without loading or saving a graph:

```bash
lexa --no-graph mcp /path/to/project
```

Example config:

```json
{
  "mcpServers": {
    "lexa": {
      "command": "/path/to/lexa",
      "args": ["mcp", "/path/to/project"]
    }
  }
}
```

## Language Support

Tree-sitter parsers: Zig, Python, Rust, TypeScript, JavaScript, Go, C, C++,
Java, Ruby, PHP.

Lightweight parsers: HCL, R, Markdown, JSON, TOML, YAML, Dart, Kotlin, Swift,
Svelte, Vue, Astro, shell, CSS, SCSS, SQL, protobuf, Fortran, LLVM IR, MLIR,
TableGen.

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## Binary Releases

GitHub Actions builds release artifacts for macOS Apple Silicon, macOS Intel,
Linux x86_64, and Windows x86_64.

To publish a GitHub Release with all binaries:

```bash
git tag v0.3.0
git push origin v0.3.0
```

Run the benchmark suite:

```bash
cargo bench --bench engine
```

For a faster local smoke benchmark:

```bash
cargo bench --bench engine -- --warm-up-time 1 --measurement-time 2 --sample-size 10
```

Smoke benchmark baseline from June 3, 2026 on a generated Rust fixture corpus:

| Benchmark | Corpus | Time |
| --- | ---: | ---: |
| `project_index/100` | 100 files | ~5.7 ms |
| `project_index/500` | 500 files | ~30.8 ms |
| `search/exact_word` | 1,000 files | ~57.6 us |
| `search/unique_token` | 1,000 files | ~192 us |
| `search/regex` | 1,000 files | ~54.1 us |
| `search/rich_scoped` | 1,000 files | ~92.9 us |
| `search/symbol_defs` | 1,000 files | ~91.5 ns |
| `search/callers` | 1,000 files | ~97.9 us |
| `incremental_edit/single_file_reindex` | 500 files | ~1.1 ms |
| `snapshot/write` | 500 files | ~6.4 ms |
| `snapshot/load_into_engine` | 500 files | ~7.6 ms |

Treat these numbers as a local regression baseline. Hardware, filesystem, and full
Criterion settings will shift absolute timings.

Lexa is ready to use. Graph format and output details may still evolve as the project grows.
