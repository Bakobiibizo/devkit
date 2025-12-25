# devkit

A unified developer workflow toolkit for Rust, Python, and TypeScript projects.

## Components

### `dev` CLI

A single-binary Rust CLI for unified developer workflows:

- Config-driven task runner (pipelines: `fmt`, `lint`, `type`, `test`, `fix`, `check`, `ci`)
- Language scaffolding + provisioning (`dev install`)
- Git flows (`dev git ...`)
- Versioning (`dev version ...`)
- `.env` helpers (`dev env ...`)
- System setup (`dev setup ...`)
- Dockerized GPU dev containers (`dev docker ...`)
- Review overlays and directory manifests (`dev review`, `dev walk`)

### `devkey` (Windows only)

A hotkey-triggered popup for quick access to environment variables and dev tasks. Press `Ctrl+;` to open.

For the authoritative spec, see `docs/spec.md`.

## Install

### From crates.io

```bash
cargo install devkit-cli
```

### From source

```bash
# Clone the repo
git clone https://github.com/bakobiibizo/devkit
cd devkit

# Install dev CLI
cargo install --path crates/dev

# (Windows only) Build devkey
cargo build --release -p devkey
# Binary at target/release/devkey.exe
```

### Prebuilt binaries

Download from [GitHub Releases](https://github.com/bakobiibizo/devkit/releases).

## Quick usage

### Tasks and pipelines

```bash
dev list
dev run <task>

dev fmt
dev lint
dev type
dev test
dev fix
dev check
dev ci

dev all <fmt|lint|type|test|fix|check|ci>
```

### Config

```bash
dev config
dev config check
dev config generate [PATH] --force
```

### Language tooling

```bash
dev install [rust|python|typescript]
dev language set <name>
```

### `.env` management

```bash
dev env [--raw]
dev env get <KEY>
dev env add <KEY> <VALUE>
dev env rm <KEY>

dev env profiles
dev env switch <PROFILE>
dev env save <NAME>

dev env check
dev env init
dev env template

dev env diff [REF]
dev env sync [REF]
```

## Docker workflow (GPU dev container)

This is designed for “build inside containers” workflows (including NVIDIA GPU containers).

### 1) Scaffold Docker files in your project

Run this in the project directory:

```bash
dev docker init
```

This generates:

- `docker/Dockerfile.core`
- `docker-compose.yml`
- `.env` (includes `CORE_IMAGE`, `UID`, `GID`)

### 2) Build the core image

`dev docker build` reads `CORE_IMAGE` from your project `.env` by default:

```bash
dev docker build
```

Override tag if needed:

```bash
dev docker build --image bakobiibizo/devkit:cuda13
```

### 3) Enter the container with an interactive shell

This is the primary “put me in the container” command:

```bash
dev docker dev
```

It runs:

- `docker compose up -d --build`
- then `docker compose exec <service> bash -l`

Options:

```bash
dev docker dev --service core
dev docker dev --no-up
```

### 4) Compose helpers

```bash
dev docker compose up build
# or detached:
dev docker compose up build -d
```

## Setup system

```bash
dev setup

dev setup run <components...>
dev setup all

dev setup status
dev setup list
dev setup config
```

## Review + walk

```bash
dev review [--output <path>] [--include-working] [--main]

dev walk [DIR] -o manifest.md --max-depth 10 --extensions .rs .py
```
