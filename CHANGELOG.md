# Changelog

## Unreleased

## 2025-12-14 - v0.2.2

- Describe the notable changes here.


### Added
- Global `--dry-run` / `--no-color` flags accepted after subcommands.
- Git workflows: `branch-create`, `branch-finalize`, and `release-pr` now execute fetch/rebase/merge/push logic, respect dry-run, and update changelogs.
- Version commands (`show`, `bump`, `changelog`) support semantic versioning, changelog updates, staging/commit/tag, and dry-run previews.
- Logging initializes a tracing subscriber to enable structured logs.
- Cargo scaffolds and deny templates now allow MPL/Unicode licenses and run `cargo +nightly udeps` by default.

### Fixed
- Addressed cargo-udeps false positives by ignoring chrono/regex/serde_json.
- Prevented release PR creation when no commits exist between base and head.
- Ensured config generation overwrites only with `--force` and summarises configs on show/check/reload.
- Avoided changelog writes before git status checks in release PR flow.

## 2025-10-09 - v0.1.1

- Describe the notable changes here.
