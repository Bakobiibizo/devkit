# AGENTS

## Identity & Preferences
- Working repo: `tools/dev`
- Owner request (2025-10-07): review `docs/` and begin Rust CLI development per spec.

## Workflows & Conventions
- Follow spec in `docs/spec.md`; keep verbs/pipelines aligned with config example.
- Record design/implementation notes inline or in this log for continuity.

## TODO (2025-10-07)
- [x] Outline initial crate structure (`cli`, `config`, `tasks`, etc.).
- [x] Implement CLI parsing scaffold with clap enums.
- [x] Draft config data models using `serde`/`toml_edit`.
- [x] Set up module tree and placeholder files per spec.
- [x] Wire task indexing into command execution with dry-run logging.
- [x] Implement config discovery/override mechanics (`--file`, default path).
- [x] Support updating default language via `dev language set`.
- [x] Flesh out `.env` helper read/write workflows.
- [x] Implement `dev install` execution flow (language detection, scaffolds, tooling orchestration).

## Lessons Learned
- Example config in `docs/example.config.toml` demonstrates command flattening with string refs; loader must support both array and string variants.
- CLI now exposes `dev install [<language>]`, defaulting to `--language` or config default when omitted.
- Runner now loads config, flattens tasks, and streams command status with `[ok]/[warn]` markers for clear feedback.
- `.env` commands locate nearest file (cwd → git root), create if missing, and preserve comments/ordering on write.
- `dev language set <name>` now writes through toml_edit so user config keeps comments intact.
- `dev install` scaffolds language templates (Rust/Python/TS) and runs optional provisioning commands from config.
- Templates are embedded via `rust-embed`; installers emit `[ok]` status as they run provisioning commands.
- `dev config` now summarises parsed config, supports `generate`, and reloads using embedded example.
- Added `--force` overwrite option for `dev config generate` and installer streams stdout/stderr live.
- Rust deny template now whitelists MPL-2.0 and Unicode licenses so `cargo deny check` passes by default.
- Git flows (`branch-create`, `branch-finalize`, `release-pr`) and version commands are fully implemented; logging installs a tracing subscriber on first use.
- `dev git branch-create` now runs the documented git workflow (fetch, checkout base, pull, create branch, optional push).
- Extended env management (2025-12): `--raw` flag, `get`, profiles (`profiles`/`switch`/`save`), validation (`check` with `[env]` config), templates (`init`/`template`), and diff/sync commands.
