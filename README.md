# devkit

Devkit is a Rust workspace that ships the `dev` CLI: a single binary for common project workflows across Rust, Python, TypeScript, and Elixir projects.

The CLI reads a devkit config, indexes tasks and language pipelines, and exposes consistent commands for running checks, managing `.env` files, scaffolding language tooling, driving git/release flows, generating review context, running setup components, and launching configured agents.

## Fresh Clone Quick Start

```bash
git clone https://github.com/bakobiibizo/devkit
cd devkit

# Build and verify the workspace.
cargo build --workspace
cargo test --workspace

# Install the CLI from this checkout.
cargo install --path crates/dev

# Create a starter config, then inspect what the CLI sees.
dev config generate
dev config show
dev list
dev --help
```

To run without installing:

```bash
cargo run -p devkit-cli -- --help
cargo run -p devkit-cli -- config check
```

## Core Usage

```bash
# Configured tasks and language pipelines
dev list
dev run <task>
dev fmt
dev lint
dev type
dev test
dev fix
dev check
dev ci
dev all check

# Language defaults and scaffolding
dev language set rust
dev install [rust|python|typescript|elixir]

# Summarized execution and background agents
dev summary exec -- cargo test --workspace
dev summary run all_check
dev agent run default --prompt "Fix the failing tests"
dev agent list
dev agent status <job-id>
```

`dev run <task>` streams raw task output. The first-class verbs and `dev summary ...` capture noisy command output and print a compact summary.

## Command Families

The current command surface is:

- `list`, `run`, `start`, first-class verbs, and `all` for configured tasks and pipelines.
- `config`, `language`, and `install` for devkit configuration and language setup.
- `env` for `.env` files, profiles, validation, templates, diffs, and sync.
- `git` and `version` for branch workflows, release PRs, version bumps, tags, and changelog output.
- `docker`, `setup`, and `os` for container scaffolds, host setup components, inference repo setup, and platform config overlays.
- `review` and `walk` for Markdown review reports and LLM-ready directory manifests.
- `summary` and `agent` for summarized command execution and configured coding agents.
- `vault` for 1Password CLI-backed secrets.
- `research` for internal research workspace scaffolding. It is hidden from top-level help but documented in `docs/USAGE.md`.

## Git Workflow

```bash
dev git branch-create feature/docs
dev git branch-create feature/docs --from main --push
dev git branch-finalize --delete
dev git release-pr patch --from main --to release-candidate
```

Branch commands resolve their base from the explicit flag first, then `[git].main_branch`, then `origin/HEAD`, then local `main`/`master`, then the current branch. Repositories without a remote can still create local branches.

## Setup And Docker

```bash
dev setup
dev setup status
dev setup list
dev setup run rustup uv docker
dev setup all --skip-installed

dev docker init
dev docker build
dev docker develop
dev docker compose up build -d
```

Setup components are idempotent and dry-run aware. CUDA host tooling uses validate-first detection to avoid modifying existing OEM GPU images unless explicitly requested by the component flow.

## Docs

- [`docs/USAGE.md`](docs/USAGE.md) has command examples and configuration notes.
- [`docs/spec.md`](docs/spec.md) is the functional and technical spec.
- [`docs/example.config.toml`](docs/example.config.toml) is the embedded starter config source.
