# Lexa

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)
[![MCP](https://img.shields.io/badge/MCP-ready-4b5563.svg)](#mcp)
[![Status](https://img.shields.io/badge/status-ready%20to%20use-brightgreen.svg)](#development)

Fast local code intelligence for humans and AI agents.

Lexa indexes a project into a portable graph, then answers codebase questions through a CLI or MCP server. Use it for symbol lookup, text search, file outlines, dependency tracing, hash-aware reads, and atomic line-based patches without running an HTTP daemon.

```bash
lexa index .
lexa text-search "handle_request" --scope
lexa outline src/main.rs
lexa mcp .
```

| Project | Info |
| --- | --- |
| Type | Local code intelligence CLI and MCP server |
| Graph | `.lexa/graph.lexa` by default |
| Runtime | Native Rust binary, no HTTP daemon |
| License | MIT |

## Install

```bash
cargo install --path .
```

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

By default, Lexa writes its graph to `.lexa/graph.lexa`.

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

## MCP

Start an MCP server for a project:

```bash
lexa mcp /path/to/project
```

Start without reading or writing a graph:

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

Tree-sitter parsers: Zig, Python, Rust, TypeScript, JavaScript, Go, C, C++, Java, Ruby, PHP.

Lightweight parsers: HCL, R, Markdown, JSON, TOML, YAML, Dart, Kotlin, Swift, Svelte, Vue, Astro, shell, CSS, SCSS, SQL, protobuf, Fortran, LLVM IR, MLIR, TableGen.

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Lexa is ready to use. Graph format and output details may still evolve as the project grows.
