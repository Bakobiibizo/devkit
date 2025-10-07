# Dev CLI (Rust, single binary) — Functional + Tech Spec

## Goals

* One self-contained Rust binary, easy to scp to any server.
* Dynamically reads `~/.dev/config.toml` to expose verbs and pipelines.
* Uniform verbs across languages: `fmt, lint, type, test, fix, check, ci`.
* Git flows: `branch-create`, `branch-finalize`, `release-pr`.
* Version management: bump, tag, changelog.
* Language management: `dev language <name>`, `dev install [<language>]` for scaffold + tool install.
* Env management: `dev env`, `dev env add`, `dev env rm`.
* Config management: `dev config`, `dev config check`, `dev config generate`.

## CLI surface

```
dev [GLOBAL] <command> [args]

Global:
  -C, --chdir <PATH>        Change working directory
  -f, --file <PATH>         Config path (default: ~/.dev/config.toml)
  -l, --language <NAME>     Override default_language
  -n, --dry-run             Print commands without executing
  -v, --verbose...          Verbosity (repeatable)
      --no-color            Disable color

Commands:
  list                             List tasks (grouped by language/verb)
  run <task>                       Run named task or pipeline (e.g., rust_fmt, all_check)
  fmt|lint|type|test|fix|check|ci  Run verb for current or --language
  all <verb>                       Run monorepo aggregator (fmt|lint|type|test|fix|check|ci)

  language set <NAME>              Set default language in ~/.dev/config.toml
  install [<NAME>]                 Scaffold configs + install tooling (defaults to current language)

  git branch-create <name> [--from <base>] [--push] [--allow-dirty]
  git branch-finalize <name> [--into <base>] [--delete] [--allow-dirty]
  git release-pr [--from <base>] [--to <head>] [--no-open]

  version bump <major|minor|patch|prerelease|custom <x.y.z>> [--tag] [--no-commit] [--no-changelog]
  version changelog [--since <ref>] [--unreleased]
  version show

  env                              List .env variables
  env add <KEY> <VALUE>            Add/update .env var
  env rm <KEY>                     Remove .env var

  config                           Display config
  config check                     Validate config and display its path
  config generate <PATH> [--force] Generate <PATH> from default config 
                                    (default: ~/.dev/config.toml)
  config reload                    Reparse config and reindex tasks
```

## Config format (minimal recap)

* `default_language = "rust" | "python" | "typescript"`
* `[tasks.<name>]` with `commands = [[...], ...]` or `["task_ref", ...]`
* `[languages.<name>.pipelines] fmt|lint|type|test|fix|check|ci = ["task_a", "task_b"]`
* Monorepo: `[tasks.all_fmt]`, `[tasks.all_check]`, etc.
* `[git] main_branch, release_branch, version_file, changelog`

Use `toml_edit` so comments survive round-trip edits.

## Execution model

* Composite tasks flatten into ordered command lists before execution.
* No implicit shell, run argv arrays directly. If a command contains shell syntax, run `["sh","-lc", "<cmd>"]`.
* Stream output, prefix with `[k/N] <task> :: <argv>`.
* Stop on first failure unless `allow_fail = true`.

## Git flows

* Shell out to `git` and `gh` if available.
* `branch-create`: checkout base (default `release-candidate`), fetch, rebase, create branch, push with upstream.
* `branch-finalize`: merge feature into base with `--no-ff`, push, optionally delete feature locally/remotely.
* `release-pr`: compute range, update or create `CHANGELOG.md`, open PR via `gh pr create` or GitHub API with `GITHUB_TOKEN`.

## Version management

* Detect backend from `git.version_file` or auto:

  * `pyproject.toml` → `[project].version`
  * `package.json` → `version`
  * `Cargo.toml` → `[package].version`
* `version bump` edits the file, commits unless `--no-commit`, optional tag `vX.Y.Z`.
* Changelog follows Keep a Changelog, it promotes “Unreleased” into the new version section with today’s date.

## Language installers and scaffolds

### `dev install rust`

* Ensure `rustup`, install components if missing:

  * `rustup component add rustfmt clippy`
* Optional tools if present in config `languages.rust.install`: `cargo-audit`, `cargo-deny`, `cargo-udeps` (use `+nightly` for udeps if needed).
* Drop sample files if absent:

  * `.cargo/config.toml` (incremental, target dir hints)
  * `deny.toml` (license/banlist template)

### `dev install python`

* Ensure `uv`, run `uv sync` if `pyproject.toml` exists.
* Create `ruff.toml`, `mypy.ini`, `.pre-commit-config.yaml` with sane defaults if missing.

### `dev install typescript`

* Ensure `pnpm` (or fallback to `npm`), run `pnpm install` if `package.json` exists.
* Create `eslint.config.ts`, `tsconfig.json`, `vitest.config.ts`, `.prettierrc` if missing.

Scaffold templates (safe defaults):

* `eslint.config.ts`

```ts
import tsParser from "@typescript-eslint/parser";
import tsPlugin from "@typescript-eslint/eslint-plugin";

export default [
  {
    files: ["**/*.ts", "**/*.tsx"],
    languageOptions: { parser: tsParser, ecmaVersion: "latest", sourceType: "module" },
    plugins: { "@typescript-eslint": tsPlugin },
    rules: {
      "no-unused-vars": "warn",
      "no-undef": "error",
      "@typescript-eslint/no-explicit-any": "off"
    }
  }
];
```

* `tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "types": ["vitest/globals"]
  },
  "include": ["src", "test", "tests"]
}
```

* `vitest.config.ts`

```ts
import { defineConfig } from "vitest/config";
export default defineConfig({ test: { globals: true, environment: "node" } });
```

* `ruff.toml`

```toml
line-length = 100
target-version = "py311"
extend-select = ["I", "UP", "PL", "RUF"]
ignore = ["E501"]
```

* `mypy.ini`

```ini
[mypy]
python_version = 3.11
strict = False
warn_unused_ignores = True
disallow_untyped_defs = False
```

* `deny.toml` (snippet)

```toml
[licenses]
unlicensed = "deny"
allow = ["MIT", "Apache-2.0", "BSD-3-Clause", "ISC"]
```

## .env management

* File path resolution:

  * Look for `.env` in CWD, else project root (git top). Create if missing.
* `dev env` prints sorted keys, masks values unless `--raw`.
* `dev env add KEY VALUE` inserts or replaces exactly one line (`KEY=VALUE`), preserves order/comments around.
* `dev env rm KEY` removes the line if present.
* Use a tiny parser: read lines, allow `# comments`, `KEY=VALUE`, no multi-line.

## Project layout (single crate)

```
dev-cli/
  Cargo.toml
  src/
    main.rs
    cli.rs            // clap subcommands
    config.rs         // load/validate with serde + toml_edit
    tasks.rs          // indexing, flattening, cycle detection
    runner.rs         // exec, dry-run, logging
    gitops.rs         // branch-create/finalize, release-pr
    versioning.rs     // bump/tag/changelog backends
    envfile.rs        // .env read/write
    scaffold/
      mod.rs
      rust.rs
      python.rs
      typescript.rs
    util.rs
    logging.rs
  templates/          // embedded templates (see below)
```

## Dependencies

* `clap` + `clap_derive` for CLI.
* `anyhow` for errors.
* `serde`, `serde_json`, `toml`, `toml_edit` for config.
* `regex` for simple parsing.
* `duct` or native `std::process::Command` for processes.
* `dirs` or `camino` for paths.
* `chrono` for dates in changelog.
* `git2` optional, but prefer shelling out to `git` and `gh` to keep behavior predictable.
* `include_dir` or `rust-embed` to ship template files inside the binary.

## Template embedding

Use `rust-embed`:

```rust
#[derive(rust_embed::RustEmbed)]
#[folder = "templates"]
struct Templates;
```

Then write files if missing.

## Error handling and UX

* Every actionable step supports `--dry-run`.
* Clear failure messages with the exact argv and exit code.
* Exit codes:

  * 0 success, 1 generic failure, 2 config/schema error, 3 git state error.

## Minimal data models (serde)

```rust
#[derive(serde::Deserialize)]
pub struct DevConfig {
    pub default_language: Option<String>,
    pub tasks: Option<std::collections::BTreeMap<String, Task>>,
    pub languages: Option<std::collections::BTreeMap<String, Language>>,
    pub git: Option<GitConfig>,
}

#[derive(serde::Deserialize)]
pub struct Task {
    pub commands: Vec<toml::Value>, // either ["ref"] or ["sh","args"] arrays
    #[serde(default)]
    pub allow_fail: bool,
}

#[derive(serde::Deserialize)]
pub struct Pipelines {
    pub fmt: Option<Vec<String>>,
    pub lint: Option<Vec<String>>,
    pub r#type: Option<Vec<String>>,
    pub test: Option<Vec<String>>,
    pub fix: Option<Vec<String>>,
    pub check: Option<Vec<String>>,
    pub ci: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
pub struct Language {
    pub install: Option<Vec<Vec<String>>>,
    pub pipelines: Option<Pipelines>,
}

#[derive(serde::Deserialize)]
pub struct GitConfig {
    pub main_branch: Option<String>,
    pub release_branch: Option<String>,
    pub version_file: Option<String>,
    pub changelog: Option<String>,
}
```

## Command runner rules

* Resolve task name to a flattened list of `argv` commands:

  * `["ref"]` expands by recursively inlining referenced task.
  * `["cargo","fmt","--","--check"]` executes directly.
* Detect cycles with DFS stack.
* If `--language` is set, verbs map to that language’s pipelines; else `default_language`.

## Example main skeleton

```rust
// src/main.rs
mod cli;
mod config;
mod tasks;
mod runner;
mod gitops;
mod versioning;
mod envfile;
mod scaffold;
mod logging;

fn main() -> anyhow::Result<()> {
    let app = cli::build();
    app.run()
}
```

```rust
// src/cli.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dev")]
pub struct App {
  #[arg(short='f', long="file")]
  pub file: Option<std::path::PathBuf>,
  #[arg(short='l', long="language")]
  pub language: Option<String>,
  #[arg(short='n', long="dry-run")]
  pub dry_run: bool,
  #[arg(short='v', long="verbose", action=clap::ArgAction::Count)]
  pub verbose: u8,
  #[command(subcommand)]
  pub cmd: Cmd
}

#[derive(Subcommand)]
pub enum Cmd {
  List,
  Run { task: String },
  Fmt, Lint, Type, Test, Fix, Check, Ci,
  All { verb: String },
  Install { language: Option<String> },
  Language { #[command(subcommand)] lang: LangCmd },
  Git { #[command(subcommand)] git: GitCmd },
  Version { #[command(subcommand)] ver: VerCmd },
  Env { #[command(subcommand)] env: EnvCmd },
  Config { #[command(subcommand)] cfg: ConfigCmd },
}

#[derive(Subcommand)]
pub enum LangCmd {
  Set { name: String },
}

#[derive(Subcommand)]
pub enum ConfigCmd {
  Show,
  Check,
  Generate { path: Option<std::path::PathBuf>, #[arg(long="force")] force: bool },
  Reload,
}

// … GitCmd, VerCmd, EnvCmd enums …
```

## MVP milestones

1. Load config, list tasks, run single task, composite flattening, dry-run.
2. Verb dispatch, `all_*` aggregators, exit codes, logging polish.
3. Language installers with embedded templates.
4. Git flows and version bump + changelog.
5. `.env` add/rm/list.
6. Ship a `Makefile` or `justfile`, push to GitHub. Then, immediately add a CI that runs `dev ci --all` on PRs.
