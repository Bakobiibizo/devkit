# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Workflow Rules

- **Always perform a hand-off before going into planning mode.** Planning mode wipes your context before you start building so you will lose any context not recorded.

## Project Overview

Devkit is a Rust workspace containing one crate:
- **`dev` (crates/dev)**: A unified CLI for developer workflows across Rust, Python, TypeScript, and Elixir projects

## Build & Development Commands

```bash
# Build entire workspace
cargo build --workspace

# Install dev CLI locally
cargo install --path crates/dev

# Run without installing
cargo run -p devkit-cli -- <args>

# Tests
cargo test --workspace

# Check, lint, format
cargo check --workspace
cargo clippy --workspace
cargo fmt --workspace

# Security/license auditing (requires tools installed)
cargo audit
cargo deny check
cargo +nightly udeps
```

## Architecture

### dev CLI (crates/dev)

The CLI follows a config-driven task runner pattern:

```
src/
├── main.rs          # Entry: logging init -> cli parse -> dispatch
├── cli.rs           # Clap definitions for all commands
├── cli_help.rs      # Dynamic help summaries from active config
├── dispatch.rs      # Context/config resolution and command routing
├── config.rs        # Loads ~/.dev/config.toml with serde + toml_edit
├── tasks.rs         # Task indexing, flattening, cycle detection
├── envfile.rs       # .env file read/write with profile support
├── gitops.rs        # Git branch workflows, release PRs
├── versioning.rs    # Version bump, changelog, tagging
├── dockergen.rs     # Docker file generation and compose helpers
├── review.rs        # Git diff → markdown review overlay
├── vault.rs         # 1Password CLI integration
├── walk.rs          # Directory manifests for LLM context
├── templates.rs     # rust-embed template handling
├── commands/        # Per-family handlers: task, config, env, git, setup, docker,
│                    # review, walk, summary, agent, research, vault, os, version
├── core/            # Shared exec, git, changelog, and output helpers
├── scaffold/        # Language-specific scaffolding (rust, python, typescript, elixir)
└── setup/           # 7 setup modules; 15 named components in setup/component.rs
```

**Key patterns:**
- Config uses `toml_edit` to preserve comments on write
- Tasks can reference other tasks; flattening resolves refs with cycle detection
- Templates embedded via `rust-embed` from `templates/` directory
- Setup components implement `detect() -> InstallState` and `install()` contract
- Setup component names: `system_packages`, `git_lfs`, `uv`, `rustup`, `node`, `pnpm`, `pm2`, `docker`, `nvidia_container_runtime`, `cuda_toolkit_host`, `zoxide`, `atuin`, `ngrok`, `rm_guard`, `op`
- Output uses `[ok]/[warn]/[error]` markers for feedback

## Config Structure

The CLI reads from `~/.dev/config.toml`:
- `default_language` - rust/python/typescript
- `[tasks.<name>]` - Commands as arrays or task refs
- `[languages.<name>.pipelines]` - Maps verbs (fmt/lint/type/test/fix/check/ci) to tasks
- `[git]` - Branch naming, version file location
- `[env]` - Required/optional env vars for validation
- `[setup]` - Default components, skip list, versions

See `docs/example.config.toml` for full reference.

## CLI Verb Dispatch

Standard verbs dispatch through language pipelines:
```bash
dev fmt              # runs [languages.<default>.pipelines.fmt]
dev lint -l python   # runs python lint pipeline
dev all check        # runs all_check task (all languages)
```

## Testing

Container-based test framework in `.test/`:
```bash
# Run basic tests in Docker
.test/run-basic-tests.sh

# Manual testing
cargo run -p devkit-cli -- setup status
cargo run -p devkit-cli -- config check
```

## Key Dependencies

- `clap` with derive - CLI parsing
- `serde` + `toml` + `toml_edit` - Config handling
- `rust-embed` - Template embedding
- `anyhow` - Error handling
- `tracing` - Logging
