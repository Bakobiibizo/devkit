# Changelog

## Unreleased

## 2026-07-13 - v0.5.0

### Added

- Added `dev guard`, a fast diff-only regression check with repository-owned regex and path rules, compact GitHub annotations, and separate blocking and advisory severities.
- Added a low-noise default guard policy for skipped work, swallowed failures, fail-open authorization, verification bypasses, unsafe shell tracing, and replacement SQL.
- Added a required CI guard job that compares only recent commits with their merge base.

### Changed

- Guard policies are read from the base revision by default so a proposed change cannot weaken its own check.
- Bumped `devkit` and `devkit-cli` to 0.5.0.

## 2026-06-12 - v0.4.0

### Added

- Added integration coverage for config generation/checking, task flattening, `.env` workflows, git branch flows, and version/changelog updates.
- Added Docker Hub publishing workflow for tagged multi-arch `devkit-core` images.
- Documented the aarch64 / GB10 inference image and `dev docker init` usage.
- Prepared crates.io package metadata for adopter installs.

### Changed

- Repositioned the README for new adopters with a 60-second quick start, use cases, and the `agntctl` companion link for LLM-driven workflows.
- Removed non-core command families from the public CLI surface so `devkit` focuses on task pipelines, env management, git/release flows, setup, Docker scaffolding, review, and walk.
- Bumped `devkit-cli` to 0.4.0 for release preparation.

## 2026-02-06 - v0.3.1

### Added

- Added Python scaffold improvements that install `uv`, `ruff`, and `mypy`.
- Added default Python CI pipeline coverage for `pre-commit` hooks after lint, type, and test.

## 2025-12-21 - v0.3.0

### Added

- Added native Markdown review reports from git diffs.
- Added directory manifest generation for LLM-ready repository context.
- Added system setup components for platform-aware developer machine provisioning.
- Added Docker scaffolding and inference-oriented container workflows.

## 2025-12-14 - v0.2.2

### Added

- Added global `--dry-run` and `--no-color` flags accepted after subcommands.
- Added git workflows for branch creation, branch finalization, and release PR preparation.
- Added version commands for semantic version display, bumps, changelog updates, commits, and tags.
- Added structured tracing subscriber initialization.
- Added Cargo scaffolds and deny templates with MPL and Unicode license allowances.

### Fixed

- Addressed cargo-udeps false positives by ignoring known runtime and serialization dependencies.
- Prevented release PR creation when no commits exist between base and head.
- Ensured config generation overwrites only with `--force`.
- Avoided changelog writes before git status checks in release PR flow.

## 2025-10-09 - v0.1.1

### Added

- Added the initial `dev` CLI for config-driven tasks, language pipelines, and project scaffolding.
