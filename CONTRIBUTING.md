# Contributing

Thanks for helping improve Lexa. This project is ready to use and moving quickly, so small, focused contributions are the easiest to review.

## Getting Started

```bash
git clone git@github.com:anvia-hq/lexa.git
cd lexa
cargo build
cargo test
```

## Development Checks

Before opening a pull request, run:

```bash
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

For release-build or performance-sensitive changes, also run:

```bash
cargo build --release
```

## Contribution Guidelines

- Keep changes scoped to one feature, fix, or cleanup.
- Follow the existing Rust style and module boundaries.
- Add or update tests for parser behavior, indexing behavior, patch behavior, or bug fixes.
- Avoid committing generated output such as `target/`, `.lexa/`, or local editor files.
- Update `README.md` or `skill/SKILL.md` when user-facing commands, MCP tools, or workflows change.

## Pull Requests

A good pull request includes:

- A short summary of what changed.
- Why the change is needed.
- Tests or checks that were run.
- Any known limitations or follow-up work.

## Reporting Issues

When reporting a bug, include:

- The Lexa version or commit.
- The command or MCP tool used.
- The expected result.
- The actual result.
- A small reproduction case when possible.

For feature requests, describe the workflow you are trying to improve and why the current commands are not enough.

## Code of Conduct

Participation in this project is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).
