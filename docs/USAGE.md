# Dev CLI Usage Guide

This guide covers the `dev` command surface as implemented by `crates/dev`.

## Install And First Run

```bash
git clone https://github.com/bakobiibizo/devkit
cd devkit
cargo build --workspace
cargo install --path crates/dev

dev config generate
dev config check
dev list
dev --help
```

Run from source with `cargo run -p devkit-cli -- <args>`.

Global flags:

- `-C, --chdir <path>` runs from another directory.
- `-f, --file <path>` selects a config file.
- `--project <name>` selects a configured project.
- `-l, --language <name>` overrides `default_language`.
- `-n, --dry-run` prints planned commands where supported.
- `-v, --verbose` increases logging verbosity.
- `--no-color` disables colored output.

## Command Overview

The CLI exposes the core devkit command families:

- `list`, `run`, `start`, `all`
- `install`, `language`
- `git`, `version`
- `env`, `config`
- `setup`, `docker`
- `review`, `walk`

The first-class verbs `fmt`, `lint`, `type`, `test`, `fix`, `check`, and `ci` are hidden clap commands normalized through the dynamic help layer and dispatch to language pipelines.

## Tasks And Pipelines

```bash
dev list
dev run rust_fmt
dev start --port 5173

dev fmt
dev lint
dev type
dev test
dev fix
dev check
dev ci
dev all check
```

`dev run <task>` streams raw configured task output. First-class verbs use the configured language pipeline and summarized subprocess reporting. `dev all <verb>` runs the monorepo aggregate task for a verb.

## Configuration

```bash
dev config
dev config show
dev config path
dev config check
dev config generate [path] [--force]
dev config reload
dev config add <name> -- <command...>
dev config add <name> --append -- <command...>
```

Config discovery checks the explicit `--file` first, then project-local `.dev/config.<os>.toml`, `.dev/config.toml`, legacy `tools/dev/config.<os>.toml`, legacy `tools/dev/config.toml`, and finally `~/.dev/config.toml`. If only the home default is missing, the CLI can proceed with an empty config for workflows that do not require configured tasks.

## Language Setup

```bash
dev language set rust
dev install
dev install python --force
dev install typescript --no-scaffold
dev install elixir
```

`dev install` defaults to `--language` or `default_language`, writes language scaffold files unless `--no-scaffold` is set, and runs optional provisioning commands from config.

## Environment Files

```bash
dev env
dev env --raw
dev env get DATABASE_URL
dev env add DATABASE_URL postgres://localhost/dev
dev env rm DATABASE_URL

dev env profiles
dev env switch staging
dev env save staging

dev env check
dev env init
dev env template
dev env diff [.env.example]
dev env sync [.env.example]
```

`.env` resolution walks from the current directory to the git root. Writes preserve comments and ordering. `dev env check` uses `[env].required` and `[env].optional` from config.

## Git And Versioning

```bash
dev git branch-create feature/docs
dev git branch-create feature/docs --from main --push
dev git branch-finalize
dev git branch-finalize feature/docs --into main --delete
dev git release-pr patch --from main --to release-candidate
dev git release-pr minor --no-open

dev version show
dev version bump patch
dev version bump minor --tag
dev version bump patch --custom 1.2.3 --no-commit
dev version changelog --unreleased
dev version changelog --since v1.2.0
```

`branch-create` and `branch-finalize` resolve their base from the command flag, `[git].main_branch`, `origin/HEAD`, local `main`/`master`, then the current branch. They skip remote fetch/pull work when no `origin` remote exists. `release-pr` requires a bump level, updates version/changelog state, pushes the release branch, and uses `gh` to create the PR unless `--no-open` is set.

## Setup And Docker

```bash
dev setup
dev setup status
dev setup list
dev setup config
dev setup run rustup uv docker --skip-installed
dev setup all --skip-installed
dev setup inference comfyui --dest ~/repos/inference/dev-comfyui

dev docker init
dev docker build
dev docker develop
dev docker dev --service core --no-up
dev docker compose up build -d
```

Setup has 14 named components: `system_packages`, `git_lfs`, `uv`, `rustup`, `node`, `pnpm`, `pm2`, `docker`, `nvidia_container_runtime`, `cuda_toolkit_host`, `zoxide`, `atuin`, `ngrok`, and `rm_guard`. Dependencies are resolved unless `--no-deps` is set. Docker scaffolding writes `docker/Dockerfile.core`, `docker-compose.yml`, and `.env` entries for `CORE_IMAGE`, `UID`, and `GID`.

## Review And Walk

```bash
dev review
dev review --main --output review.md
dev review --include-working

dev walk
dev walk crates/dev -o manifest.md --extensions .rs .toml
dev walk . --no-content --max-depth 4
```

`dev review` produces a Markdown code-review overlay from staged diffs, working-tree diffs, or comparison to the main branch. `dev walk` creates an LLM-ready directory manifest and includes file contents by default.

## Verb Summaries

First-class verbs capture command output and print a compact summary. Configure that behavior with `[summary]`:

```toml
[summary]
shell = "bash"
max_output_bytes = 65536
tail_bytes = 12288
# llm_command receives a prompt on stdin and writes a summary on stdout.
# llm_command = "your-llm-summarizer"
```

## Further Reading

- [`docs/spec.md`](spec.md) for the functional and technical spec.
- [`docs/example.config.toml`](example.config.toml) for the embedded starter config.
