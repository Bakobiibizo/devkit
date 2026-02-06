# Dev CLI Usage Guide

This guide introduces the top-level workflows provided by the `dev` binary and explains how to configure and operate the tool effectively.

## Prerequisites

- Rust toolchain with cargo available on `PATH`.
- (Optional) [`gh`](https://cli.github.com) installed for release PR automation.
- Default config at `~/.dev/config.toml` (generate one with `dev config generate`).

## Quick Start

```bash
# Generate example config (~/.dev/config.toml by default)
dev config generate

# Inspect the current configuration
dev config show

# Run the lint workflow (language inferred from config)
dev lint
```

Global flags:

- `--dry-run` prints commands without executing.
- `--language <name>` overrides the default language.
- `--no-color` disables coloured output.
- `-C <path>` runs commands from another directory.

## Core Workflows

### Task Execution

- `dev list` – list known tasks and pipelines.
- `dev run <task>` – run a raw task defined in config.
- `dev fmt|lint|test|check|ci` – run the language pipeline.
- `dev all <verb>` – run `fmt`, `lint`, etc. across every configured language.

### Configuration Helpers

- `dev config show|check` – display a summary and validate the config.
- `dev config generate [path] [--force]` – write the embedded example config.
- `dev config reload` – reparse the config and print a summary.

### Environment File Utilities

- `dev env` – list environment variables discovered in the nearest `.env` file.
- `dev env add KEY VALUE` – add or update a variable.
- `dev env rm KEY` – remove a variable.

### Language Installers

- `dev install [language]` – scaffold language-specific config files and run optional provisioning commands defined in config. Supports `--dry-run` to preview actions.

## Git Automation

- `dev git branch-create <name> [--from <base>] [--push] [--allow-dirty]`
  - Fetches & rebases the base branch, checks out the new branch, and (optionally) pushes with upstream tracking.
- `dev git branch-finalize [<name>] [--into <base>] [--delete] [--allow-dirty]`
  - Merges the feature branch into the base branch with `--no-ff`, pushes, and optionally deletes local & remote branches. Defaults to the current branch name.
- `dev git release-pr [--from <base>] [--to <head>] [--no-open]`
  - Ensures a clean worktree, fetches and rebases the release branch, updates the changelog, pushes upstream, and opens a GitHub PR using `gh`. Skips PR creation if there are no commits between base and head.

## Version Management

- `dev version show` – print the current package version (defaults to `Cargo.toml`).
- `dev version bump <major|minor|patch|prerelease> [--custom <x.y.z>] [--tag] [--no-commit] [--no-changelog]`
  - Updates the manifest, changelog, stages changes, and optionally commits/tags.
- `dev version changelog [--since <tag>] [--unreleased]` – display commit summaries for the requested range.

## Logging

`dev` installs a `tracing` subscriber on first use. Configure verbosity via `RUST_LOG`, e.g. `RUST_LOG=dev=debug`.

## FAQ

- **How do I overwrite the config?** Use `dev config generate --force`.
- **How do I run workflows without mutating files?** Add `--dry-run` anywhere on the command line.
- **How do I install language scaffolds?** Run `dev install` (optionally specify `rust`, `python`, or `typescript`).

## Further Reading

- [`docs/spec.md`](spec.md) – full technical specification.
- [`docs/example.config.toml`](example.config.toml) – detailed configuration reference.
