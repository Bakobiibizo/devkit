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
- [ ] Wire task indexing into command execution with dry-run logging.
- [ ] Implement config discovery/override mechanics (`--file`, default path).
- [ ] Flesh out `.env` helper read/write workflows.
- [ ] Implement `dev install` execution flow (language detection, scaffolds, tooling orchestration).

## Lessons Learned
- Example config in `docs/example.config.toml` demonstrates command flattening with string refs; loader must support both array and string variants.
- CLI now exposes `dev install [<language>]`, defaulting to `--language` or config default when omitted.
