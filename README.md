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

## Quick Start

```bash
lexa index .
lexa text-search "handle_request" --scope
lexa symbol-search "createAgent"
lexa outline src/main.rs
lexa audit
lexa mcp .
```

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
