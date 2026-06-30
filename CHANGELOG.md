# Changelog

## Unreleased

### Added

- Added agent retrieval and CLI output format benchmark coverage.
- Added the Astro and Tailwind website with Guide, MCP, Skill, Benchmark, and Changelog pages.

### Changed

- Improved agent-oriented TOON output and audit ranking.
- Improved brief source ranking for more useful task context.
- Reduced structured text token overhead with flattened success metadata, shared path roots, direct path arrays, trimmed search text, conditional next steps, and triage-first audit output.
- Refreshed the public benchmark docs and site page to cover retrieval, CLI output format, edit safety, MCP session, and maintenance suites.

## v0.8.0 - 2026-06-25

### Added

- Added a canonical MCP `TOOL_SPECS` table plus generated `skill/SKILL.md` and `docs/tools.md` workflows.
- Added `xtask gen-skill` for regenerating MCP tool documentation from `TOOL_SPECS`.

### Changed

- `mcp::tools()` now consumes `TOOL_SPECS`, and MCP `tools/list` descriptions use short summaries.

### Tests

- Added invariant tests for MCP tool specs and generated tool-list shape.

## v0.7.0 - 2026-06-22

### Changed

- Rewrote all 22 MCP tool descriptions in `src/mcp.rs::tools()` from terse one-liners into 2-3 sentence guidance covering when to use each tool, what it returns, and key arguments, helping agents pick the right tool more reliably.
- Synced the same descriptions verbatim into the MCP tools table in `skill/SKILL.md` (also added the previously missing `symbol_search`, `reindex`, and `clear_index` rows and reordered rows to match `src/mcp.rs`).

## v0.6.7 - 2026-06-21

### Fixed

- Resolved default graph paths relative to the project root so `lexa index /path/to/project` writes the graph under that project by default.
- Prevented `--no-graph` edit, create, index, and watch flows from persisting snapshots.
- Made persisted `patch` and `create` fail before mutating files when no graph exists, avoiding accidental one-file snapshots.
- Made `audit` fail on empty indexes instead of reporting a clean result.
- Made `watch` fail startup when an existing graph cannot be loaded.

## v0.6.6 - 2026-06-18

### Fixed

- Ignored hidden project paths during MCP watcher refreshes so `.lexa/mcp.log` and other internal files do not trigger repeated graph updates.

## v0.6.5 - 2026-06-17

### Added

- Added `lexa mcp --log-file <path>` to append MCP startup and runtime diagnostics to a plain-text log file.

## v0.6.4 - 2026-06-12

### Fixed

- Resolved TypeScript ESM runtime import specifiers such as `./module.js` to matching `.ts`, `.tsx`, `.mts`, and `.cts` source files.
- Resolved local Vite-style resource imports with query or hash suffixes such as `styles.css?url`.

## v0.6.3 - 2026-06-11

### Fixed

- Rejected oversized MCP stdio messages before allocating request bodies.
- Made snapshot loading replace existing engine state instead of merging stale indexes.

### Changed

- Removed confirmed unused engine and walker helpers.

## v0.6.2 - 2026-06-10

### Tests

- Expanded regression coverage for MCP read, patch, create, status, recent, and changes flows.
- Added focused coverage for edit/create safety paths, pipeline output behavior, output helpers, CLI line ranges, and parser fixtures.

## v0.6.1 - 2026-06-04

### Fixed

- Fixed a `clap` argument-id collision that caused `lexa upgrade` to panic when the global `--version` flag and upgrade positional version argument shared the same internal id.

## v0.6.0 - 2026-06-04

### Added

- Added top-level `symbol_search` for fuzzy symbol discovery when exact `symbol_defs` is too strict.
- Added `reindex` and `clear_index` MCP tools for explicit graph recovery.
- Added architecture cycle detection to structural audit output.
- Added `files` filtering by path, glob, language, and line-count bounds.

### Changed

- Removed the legacy MCP `pipeline.query` argument; pipeline now accepts `pipeline` or `steps`.
- Made `brief` explicit about its scope as a context bundle for symbols, paths, and scoped keywords rather than natural-language QA.
- Improved `brief` ranking and body extraction for relevant definitions.
- Made `lexa index` print a lightweight branded banner in interactive terminals.
- Moved CLI upgrade/version-check code into a dedicated module and centralized shared output formatting.

### Fixed

- Prevented invalid graph snapshots from silently loading as an empty index and producing misleading command results.
- Made `audit` refuse to run when no files are indexed.
- Added header-first snapshot validation so incompatible graph versions fail before payload decoding.
- Cleaned `outline` output by keeping imports out of the symbol list and improving missing-file/config error messages.
- Improved JSON outline classification consistency and removed an avoidable package manifest parser unwrap.
- Named `brief` scoring weights to make future ranking changes easier to review.

### Tests

- Added regression coverage for snapshot header validation, graph-loading behavior, pipeline schema cleanup, fuzzy symbol search, outline import filtering, and parser edge cases.

## v0.5.1 - 2026-06-04

### Fixed

- Resolved local asset imports for SVG/PNG provider logos and other known asset files without indexing binary bytes.
- Returned clear metadata stubs from `read` for known binary assets instead of reporting them as missing.
- Normalized TypeScript imports in outlines and dependency data to module specifiers such as `./assets/logo.svg`.
- Avoided TypeScript outline false positives where exported object or string values could be misclassified as imports.
- Made Unix upgrades install through a staged binary and atomic move, avoiding macOS `Killed: 9` failures after in-place replacement.

### Improved

- Made `brief` prefer relevant symbol definitions before generic snippets or call sites.
- Improved `brief` natural-query handling with identifier, path, and phrase candidates.
- Ranked callable definitions such as `useTerminalSession` and `createProjectAgent` above related type aliases when both match the same concept.
- Bounded large `brief` symbol bodies to 120 lines.

### Tests

- Added regression coverage for asset import resolution, metadata-only asset reads, TypeScript import normalization, `brief` definition ranking, phrase/path-based `brief` lookup, and large symbol body truncation.
- Verified the Unix installer path with a temporary install directory.

## v0.5.0 - 2026-06-04

### Changed

- Made MCP structured content opt-in to reduce duplicated tool output by default.

## v0.4.2 - 2026-06-04

### Changed

- Clarified audit verification limits and reinforced that structural audit output does not replace build, typecheck, lint, or test verification.

## v0.4.1 - 2026-06-04

### Fixed

- Detected unresolved local TypeScript imports.
- Fixed Windows installer ZIP layout handling.
- Read MCP stdin framing as bytes to avoid non-UTF-8 input failures.

## v0.4.0 - 2026-06-03

### Added

- Added MCP graph freshness checks and watcher support.

## v0.3.0 - 2026-06-03

### Added

- Added the first audit command implementation.
- Added scoped audit strict mode.
- Added audit configuration.
- Added dead-code audit candidates.

### Improved

- Refined audit reporting and release workflow behavior.

## v0.2.0 - 2026-06-03

### Added

- Added the Lexa binary upgrade command.

### Improved

- Improved import dependency resolution.
- Restricted release workflow execution on pull requests.

## v0.1.0 - 2026-06-03

### Fixed

- Fixed release publishing without checkout.
