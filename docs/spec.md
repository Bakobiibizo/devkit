# Dev CLI Functional And Technical Spec

## Goals

- Ship one self-contained Rust binary that is easy to copy to a workstation or server.
- Read devkit TOML config dynamically and expose configured tasks, language pipelines, setup defaults, and validation rules.
- Provide uniform language verbs: `fmt`, `lint`, `type`, `test`, `fix`, `check`, and `ci`.
- Keep core workflows non-interactive, scriptable, idempotent where possible, and dry-run aware.
- Preserve config comments when the CLI edits TOML.

## CLI Surface

```text
dev [GLOBAL] <command> [args]

Global:
  -C, --chdir <PATH>        Change working directory before running
  -f, --file <PATH>         Use an explicit config file
      --project <NAME>      Select a named project from config
  -l, --language <NAME>     Override default_language
  -n, --dry-run             Print commands without executing where supported
  -v, --verbose...          Increase verbosity
      --no-color            Disable color

Commands:
  list
  run <task>
  start [--port <PORT>] [--prod]
  fmt | lint | type | test | fix | check | ci
  all <fmt|lint|type|test|fix|check|ci>

  config [show]
  config path
  config check
  config generate [PATH] [--force]
  config reload
  config add [NAME] [--force] [--append] -- <command...>

  language set <NAME>
  install [<NAME>] [--force] [--no-scaffold]

  env [--raw] [list]
  env get <KEY>
  env add <KEY> <VALUE>
  env rm <KEY>
  env profiles
  env switch <PROFILE>
  env save <NAME>
  env check
  env init
  env template
  env diff [REF]
  env sync [REF]

  git branch-create <NAME> [--from <BASE>] [--push] [--allow-dirty]
  git branch-finalize [NAME] [--into <BASE>] [--delete] [--allow-dirty]
  git release-pr <major|minor|patch|prerelease> [--from <BASE>] [--to <HEAD>] [--no-open]

  version show
  version bump <major|minor|patch|prerelease> [--custom <X.Y.Z>] [--tag] [--no-commit] [--no-changelog]
  version changelog [--since <REF>] [--unreleased]

  setup
  setup run [--skip-installed] [--no-deps] <COMPONENT...>
  setup inference <SERVICE> [--dest <PATH>] [--force] [--no-cache]
  setup all [--skip-installed] [--no-deps]
  setup status
  setup list
  setup config

  docker init [--force] [--base-image <REF>] [--service <NAME>]
  docker build [--image <REF>]
  docker compose up build [-d|--detach]
  docker develop [--service <NAME>] [--no-up]
  docker dev ...                    Alias for docker develop

  review [--output <PATH>] [--include-working] [--main]
  walk [DIR] [-o, --output <PATH>] [--format <FMT>] [--max-depth <N>] [--no-content]
       [--extensions <EXT...>] [--include-hidden]

```

The first-class verbs are hidden clap variants so dynamic help can summarize configured pipelines cleanly.

## Config Format

Core fields:

- `default_language = "rust" | "python" | "typescript" | "elixir"`
- `[tasks.<name>]` with `commands = [[...], ...]` or string references to other tasks.
- `[languages.<name>.pipelines]` mapping `fmt`, `lint`, `type`, `test`, `fix`, `check`, and `ci` to task names.
- `[git]` for `main_branch`, `release_branch`, `version_file`, and `changelog`.
- `[env]` for `required` and `optional` key lists.
- `[summary]` for shell and output capture limits.
- `[setup]` for default components, skipped components, and tool versions.

Config discovery order:

1. Explicit `--file`.
2. `.dev/config.<os>.toml`.
3. `.dev/config.toml`.
4. `tools/dev/config.<os>.toml`.
5. `tools/dev/config.toml`.
6. `~/.dev/config.toml`.

Missing explicit or discovered config is an error. Missing home-default config is treated as an empty config for command families that can run without configured tasks.

## Execution Model

- Composite tasks flatten into ordered command lists before execution.
- Command arrays execute directly without an implicit shell.
- Shell syntax belongs in an explicit command such as `["sh", "-lc", "..."]`.
- Raw tasks stream output with `[ok]`, `[warn]`, and `[error]` status markers.
- First-class verbs capture output and print compact summaries.
- Task execution stops on first failure unless the task marks a command as allowed to fail.

## Git Workflow

`branch-create`:

- Requires a clean worktree unless `--allow-dirty` is set.
- Resolves the base branch from `--from`, then `[git].main_branch`, then `origin/HEAD`, then local `main`/`master`, then the current branch.
- If `origin` exists, fetches and fast-forwards/rebases the base before creating the new branch.
- If no remote exists, skips remote work and creates the local branch.
- Pushes with upstream tracking only when `--push` is set.

`branch-finalize`:

- Defaults the feature branch to the current branch when `NAME` is omitted.
- Resolves the target base from `--into` using the same fallback order as branch creation.
- Merges the feature branch into the base with `--no-ff`, pushes when a remote is available, and optionally deletes the feature branch locally/remotely with `--delete`.

`release-pr`:

- Requires a bump level.
- Uses `[git].release_branch` or the `--to` override for the release head.
- Updates version/changelog state, pushes the release branch, and opens a GitHub PR with `gh` unless `--no-open` is set.

## Version Management

Version detection uses `git.version_file` first, then common manifests:

- `pyproject.toml` → `[project].version`
- `package.json` → `version`
- `Cargo.toml` → `[package].version`

`version bump` updates the manifest, updates changelog unless `--no-changelog` is set, commits unless `--no-commit` is set, and tags when `--tag` is passed. Changelog dates use the current local date.

## Language Installers

`dev install rust`:

- Ensures Rust formatting/lint components are available.
- Writes `.cargo/config.toml` and `deny.toml` templates when absent unless `--no-scaffold` is set.
- Runs optional `languages.rust.install` provisioning commands.

`dev install python`:

- Ensures `uv` when configured.
- Runs `uv sync` when appropriate.
- Writes `ruff.toml`, `mypy.ini`, `.pre-commit-config.yaml`, and `.env.example` templates when absent.

`dev install typescript`:

- Uses `pnpm` when available and falls back to npm-oriented tooling where configured.
- Writes `eslint.config.ts`, `tsconfig.json`, `vitest.config.ts`, and `.prettierrc` templates when absent.

`dev install elixir`:

- Scaffolds Elixir-oriented configuration from the embedded template set.

## Environment Management

- `.env` lookup starts in the working directory and falls back to the git root.
- `dev env` masks values unless `--raw` is passed.
- `get`, `add`, and `rm` preserve comments and ordering.
- Profiles are `.env.<name>` files, excluding `.env.example`.
- `check` validates required and optional keys from config.
- `template`, `init`, `diff`, and `sync` operate against `.env.example` by default.

## Setup System

Setup components:

- `system_packages`
- `git_lfs`
- `uv`
- `rustup`
- `node`
- `pnpm`
- `pm2`
- `docker`
- `nvidia_container_runtime`
- `cuda_toolkit_host`
- `zoxide`
- `atuin`
- `ngrok`
- `rm_guard`

Dependency rules:

- `git_lfs` depends on `system_packages`.
- `docker` depends on `system_packages`.
- `pnpm` depends on `node`.
- `pm2` depends on `node` and `pnpm`.
- `nvidia_container_runtime` depends on `docker`.

`detect()` functions must remain pure and side-effect free. Installers must be idempotent and dry-run aware. CUDA host detection uses `Installed`, `Partial`, `PresentButUnknown`, and `NotInstalled` states so existing OEM images are validated rather than overwritten.

`setup inference <service>` clones or updates `https://github.com/bakobiibizo/dev-<service>.git`, strips explicit Compose `container_name:` entries to avoid collisions, and runs `scripts/setup.sh`.

## Docker

`docker init` writes:

- `docker/Dockerfile.core`
- `docker-compose.yml`
- `.env` entries for `CORE_IMAGE`, `UID`, and `GID`

`docker build` builds `docker/Dockerfile.core` using `CORE_IMAGE` from `.env` unless `--image` is passed. `docker develop` runs compose startup unless `--no-up` is set and then opens an interactive shell in the chosen service.

## Review And Walk

`review` generates Markdown code review reports from staged diffs, working tree diffs, or branch comparison to main. `walk` generates Markdown directory manifests with file contents by default and supports extension filtering, max-depth limits, and hidden-file inclusion.

## Verb Summaries

First-class verbs run configured pipelines through the configured shell, capture bounded stdout/stderr, and print either an LLM-generated summary or a deterministic tail summary.

## Project Layout

```text
crates/dev/
  Cargo.toml
  src/
    main.rs
    cli.rs
    cli_help.rs
    dispatch.rs
    config.rs
    tasks.rs
    envfile.rs
    gitops.rs
    versioning.rs
    dockergen.rs
    review.rs
    walk.rs
    templates.rs
    commands/
      config.rs
      docker.rs
      env.rs
      git.rs
      language.rs
      review.rs
      setup.rs
      task.rs
      version.rs
      walk.rs
    core/
      changelog.rs
      exec.rs
      git.rs
      output.rs
      summarize.rs
    scaffold/
      elixir.rs
      python.rs
      rust.rs
      typescript.rs
    setup/
      component.rs
      context.rs
      cuda.rs
      docker.rs
      system.rs
      templates.rs
      tools.rs
  templates/
```

## Dependencies

- `clap` and `clap_derive` for CLI parsing.
- `anyhow` for errors.
- `serde`, `serde_json`, `toml`, and `toml_edit` for config.
- `camino` and `dirs` for paths.
- `chrono` for changelog dates.
- `rust-embed` for template embedding.
- Native `std::process::Command` helpers for subprocess execution.
