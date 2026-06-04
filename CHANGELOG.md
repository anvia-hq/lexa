# Changelog

## Unreleased

No changes yet.

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
