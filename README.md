# Lexa

Fast local code intelligence for humans and AI agents.

Lexa indexes a project into a portable graph and answers codebase questions through a CLI or MCP server. It supports symbol lookup, text search, outlines, dependency tracing, line-range reads, and atomic line-based patches without running an HTTP daemon.

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

lexa map
lexa search "handle_request"
lexa outline src/main.rs
lexa find-symbol Engine
lexa read src/main.rs -L 1-80
```

By default, Lexa writes its graph to `.lexa/graph.lexa`.

Use a custom graph path:

```bash
lexa --graph /tmp/project.graph.lexa index /path/to/project
lexa --graph /tmp/project.graph.lexa search "Parser"
```

## Commands

| Command | Purpose |
| --- | --- |
| `index <path>` | Index a project and write a graph |
| `map [path]` | Show indexed files |
| `list [path]` | List directory children |
| `glob <pattern>` | Match indexed paths |
| `find-path <pattern>` | Fuzzy path search |
| `search <query>` | Search indexed text |
| `outline <path>` | Show imports and symbols |
| `find-symbol <name>` | Find definitions |
| `find-word <word>` | Find exact word occurrences |
| `find-callers <name>` | Find call sites |
| `trace-deps <path>` | Trace parsed imports |
| `brief <task>` | Build task-focused context |
| `read <path>` | Read a file or line range |
| `patch <path> <op>` | Apply a line-based edit |
| `changes [since]` | Show session-local changes |
| `recent` | Show recently modified files |
| `status` | Show index status |
| `watch [path]` | Refresh graph on file changes |
| `pipeline <pipeline>` | Chain query operations |
| `mcp [path]` | Start MCP over stdio |

Useful search flags:

```bash
lexa search "render" --scope
lexa search --regex "render[A-Z]\\w+"
lexa search "useEffect" --path-glob "**/*.{ts,tsx}"
lexa search "TODO" --compact --paths-only
```

Safe edit example:

```bash
lexa read src/main.rs --hash
lexa patch src/main.rs replace -L 12 --if-hash <hash> --content '    println!("updated");'
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

Lightweight parsers: HCL, R, Markdown, JSON, YAML, Dart, Kotlin, Swift, Svelte, Vue, Astro, shell, CSS, SCSS, SQL, protobuf, Fortran, LLVM IR, MLIR, TableGen.

## Development

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Lexa is early-stage software. CLI and MCP names are usable, but graph format and output details may still evolve.
