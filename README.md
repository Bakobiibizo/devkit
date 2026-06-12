# devkit

`devkit` provides the `dev` CLI: a small, config-driven command runner for repeatable project workflows. It gives a repository one place to define tasks, language pipelines, `.env` handling, git/release steps, setup components, Docker scaffolding, and review/context reports, while keeping the day-to-day interface short enough to remember.

## 60-Second Quick Start

Install from crates.io:

```bash
cargo install devkit-cli
```

Create a starter config in a project and inspect the available commands:

```bash
dev config generate
dev config check
dev list
```

Run configured work through a stable command surface:

```bash
dev fmt
dev lint
dev test
dev check
dev run <task>
```

Use the local checkout when developing `devkit` itself:

```bash
cargo install --path crates/dev
cargo run -p devkit-cli -- --help
```

## When To Use It

Use `devkit` when a project has useful commands scattered across READMEs, package scripts, Makefiles, shell snippets, or team memory. A `devkit` config can flatten those commands into named tasks and language pipelines so contributors run the same checks, setup, and release steps locally and in automation.

It is a good fit for:

- repositories with Rust, Python, TypeScript, or Elixir workflows;
- projects that need consistent `fmt`, `lint`, `type`, `test`, `check`, `ci`, or custom task verbs;
- teams that want `.env` profile management, validation, templates, diffs, and sync helpers;
- release flows that benefit from scripted branch, version, changelog, and release-PR commands;
- machines that need repeatable developer setup or Docker project scaffolding;
- LLM-assisted code review where `dev review` and `dev walk` generate bounded Markdown context.

For LLM tool loops that need compact summaries of noisy commands or detached agent runs, use [`agntctl`](https://crates.io/crates/agntctl) alongside `devkit`. `devkit` owns the project workflow surface; `agntctl` owns bounded command and agent reports.

## Common Commands

```bash
# Configured tasks and pipelines
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

# Configuration and language scaffolding
dev config show
dev config reload
dev language set rust
dev install [rust|python|typescript|elixir]

# Environment files
dev env list
dev env add KEY value
dev env get KEY
dev env profiles
dev env check

# Git, versioning, and release prep
dev git branch-create feature/example
dev git branch-finalize --delete
dev git release-pr patch --from main --to release-candidate
dev version show

# Review and context reports
dev review --main --output review.md
dev walk crates/dev -o manifest.md --extensions .rs .toml
```

`dev run <task>` streams the configured command directly. First-class verbs such as `dev test` capture noisy output and print a compact status summary.

## Setup And Docker

`dev setup` installs repeatable host components from config, with dry-run support and idempotent detection:

```bash
dev setup
dev setup status
dev setup list
dev setup run rustup uv docker
dev setup all --skip-installed
```

`dev docker` creates and operates project container scaffolds:

```bash
dev docker init
dev docker build
dev docker compose up build -d
dev docker develop
```

Tagged releases publish a multi-arch Docker image for `linux/amd64` and `linux/arm64`:

```bash
docker pull bakobiibizo/devkit-core:latest
docker pull bakobiibizo/devkit-core:v0.4.0
```

The image is built from the NGC PyTorch base and includes the build toolchain, Git/Git LFS, `uv`, cache directories for Hugging Face/Torch/uv, `nvidia-ml-py` instead of the deprecated `pynvml` package, and patched `torchaudio`/`torchvision` installs for the CUDA PyTorch stack. It is intended for aarch64 inference hosts such as GB10 / DGX Spark class machines where the host GPU stack is already provisioned.

Use the published image as a project base:

```bash
dev docker init --base-image bakobiibizo/devkit-core:v0.4.0
dev docker compose up build -d
dev docker develop
```

## Documentation

- [`docs/USAGE.md`](docs/USAGE.md) has command examples and configuration notes.
- [`docs/spec.md`](docs/spec.md) is the functional and technical spec.
- [`docs/example.config.toml`](docs/example.config.toml) is the embedded starter config source.
