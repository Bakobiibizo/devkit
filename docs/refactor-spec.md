# Devkit Refactor Specification

Date: 2026-06-11
Status: draft, pending user review
Tracking: LDGR work items (`.ldgr/ldgr.db`), slugs referenced per block below.

## Goals

1. Extract the essential core of the `dev` CLI into clearly separated modules.
2. Refine the install and setup experience.
3. Remove dead code and repository cruft.
4. Break up monolithic files and simplify complexity.
5. Fill in any mocked, TODO, or placeholder implementations.
6. Clean up onboarding (fresh-clone to working-tool path).
7. Update documentation and CLI help menus to match the real command surface.

## Survey findings (2026-06-11)

- `crates/dev/src/runner.rs` is **3,182 lines** — a god-module that mixes top-level
  command dispatch with the full implementation of research, docker, env, vault,
  os, agent, summary, walk, review, setup, and process-execution helpers.
- `crates/dev/src/cli.rs` is 877 lines of clap definitions for 19 top-level commands.
- `cargo build --workspace` (the documented build command) **fails on Linux with 31
  errors** because `devkey` is Windows-only but not excluded from default workspace
  builds. This breaks the first command a new contributor runs.
- Repo root contains scaffolding cruft: `test.md` ("this is a test"), `main.py`
  (hello-world stub), `pyproject.toml`, `uv.lock`, `mypy.ini`, `ruff.toml` — Python
  tooling files in a Rust workspace with no Python source.
- `#[allow(dead_code)]` markers in `setup/context.rs` (4 sites) and
  `setup/component.rs` (1 site).
- `devkey/src/menu.rs` has a "Loading secrets..." placeholder pattern (async menu
  population) — verify it is fully wired, not a stub.
- Documentation drift:
  - `CLAUDE.md` claims 14 setup components; `src/setup/` has 8 files.
  - `CLAUDE.md` architecture tree omits `vault.rs` and the research/agent/summary
    command families.
  - `README.md`/`docs/USAGE.md`/`docs/spec.md` predate the newest commands
    (`research`, partial agent/summary coverage).
- `devkit-cli` itself compiles clean with no warnings.

## Essential core (target architecture)

The heart of the tool is the **config-driven task runner**:

```
config.toml -> config.rs -> tasks.rs (flatten/cycle-check) -> execution -> output
```

Target layout for `crates/dev/src`:

```
main.rs              # entry: logging -> cli parse -> dispatch
cli.rs               # clap definitions only (consider splitting per command family)
dispatch.rs          # thin Command -> handler routing (replaces runner.rs dispatch)
core/
├── config.rs        # config load/save (existing, moves under core)
├── tasks.rs         # task indexing, flattening, cycle detection (existing)
├── exec.rs          # run_process*, streaming, captured, format_command,
│                    # bounded_output, shell helpers (extracted from runner.rs)
└── output.rs        # [ok]/[warn]/[error] markers, summaries
commands/
├── env.rs           # env_* handlers (~400 lines out of runner.rs)
├── docker.rs        # docker_* handlers
├── research.rs      # research_* handlers
├── agent.rs         # agent run/list/status + job records (~500 lines)
├── summary.rs       # summarized execution + LLM summary (~300 lines)
├── vault.rs / os.rs / git.rs / version.rs / walk.rs / review.rs / setup.rs
└── ...
scaffold/            # unchanged
setup/               # unchanged internally; entry point moves to commands/setup.rs
```

Acceptance: no source file over ~600 lines except generated/clap code; `runner.rs`
deleted or reduced to dispatch glue; behavior identical (existing tests +
`.test/run-basic-tests.sh` pass).

## Work blocks

### Block 1 — `workspace-hygiene` (small, do first)
- Remove the `devkey` crate entirely (user decision 2026-06-11) — this also
  fixes `cargo build --workspace` failing on Linux.
- Delete root cruft: `test.md`, `main.py`, `pyproject.toml`, `uv.lock`,
  `mypy.ini`, `ruff.toml` (verified nothing references them).
- Replace the leftover Python-template CI workflow (`.github/workflows/ci.yml`
  ran ruff/mypy/pytest/uv-build and never touched the Rust code) with a Rust
  pipeline (fmt, clippy, test, build); same for `.pre-commit-config.yaml`.
- Remove devkey references from `README.md` and `CLAUDE.md`.
- Acceptance: `cargo build --workspace`, `cargo clippy`, `cargo test` succeed
  warning-free on Linux from a fresh clone.

### Block 2 — `extract-exec-core`
- Extract process-execution primitives from `runner.rs` into `core/exec.rs`
  (+ `core/output.rs`): `run_process*`, `run_external_command`,
  `format_command`, `shell_command`, `bounded_output`, `combine_output`,
  char-boundary clamps.
- Pure code motion, no behavior change.
- Acceptance: `runner.rs` shrinks ~400 lines; all tests pass.

### Block 3 — `split-runner-commands`
- Move each command family's handlers from `runner.rs` into `commands/<family>.rs`
  (env, docker, research, agent, summary, vault, os, walk, review, setup, config,
  language, git, version).
- Leave a thin `dispatch.rs`; delete `runner.rs`.
- May land as several commits (one per family) under this single work item.
- Acceptance: target layout above; no file >600 lines; tests pass.

### Block 4 — `dead-code-and-placeholders`
- Resolve all `#[allow(dead_code)]` sites: wire up or delete.
- Sweep for any remaining TODO/stub/mocked paths; implement or file a tracked
  GitHub issue per global rules.
- Acceptance: zero `allow(dead_code)`, zero untracked TODOs.

### Block 5 — `install-setup-refinement`
- Review `dev install` and `dev setup` end-to-end: component detect/install
  contract, `setup status` output, error messages, idempotency.
- Reconcile the documented component list with reality; simplify
  `setup/system.rs` (498 lines) and `setup/tools.rs` (439 lines) where possible.
- Acceptance: `dev setup status` and `dev install` behave correctly and read
  clearly on a fresh machine; component list documented accurately.

### Block 6 — `simplify-complexity`
- Pass over `cli.rs` (877), `versioning.rs` (527), `gitops.rs` (354) for
  simplification and shared-helper extraction once Blocks 2–3 settle the layout.
- Acceptance: clippy clean, reduced duplication, no behavior change.

### Block 7 — `git-workflow-redesign`
- `dev git branch-create` / `branch-finalize` are built around a hard-coded
  `release-candidate` base branch — not a normal git workflow, and cumbersome
  in practice (user feedback 2026-06-11).
- Redesign around standard git conventions: branch off the repo's default
  branch (or current HEAD) by default, with the base configurable via
  `[git]` config rather than assumed; keep `release-pr` versioning flow working.
- Simplify the ceremony: sensible defaults, fewer required arguments, clear
  errors when the worktree is dirty.
- Acceptance: `dev git branch-create <name>` works on a vanilla repo with no
  `release-candidate` branch and no extra config; existing config-driven
  overrides still respected; docs/help updated in Block 8.

### Block 8 — `onboarding-and-docs` (last — after surface stabilizes)
- Rewrite `README.md` quick-start against the post-refactor reality; verify the
  fresh-clone path works as written.
- Update `docs/USAGE.md` and `docs/spec.md` to cover all 19 commands
  (incl. research, agent, summary, vault, os).
- Fix `CLAUDE.md` architecture section (module tree, setup component count).
- Audit clap help: every command/subcommand has accurate `about`/`long_about`;
  add `after_help` examples for the major families.
- Acceptance: docs match `dev --help` output; new-contributor path verified.

## Ordering rationale

Hygiene first (unblocks clean CI on Linux), then mechanical extraction before
any simplification (so diffs stay reviewable), placeholders and setup refinement
once the code is navigable, docs last so they describe the final state.

## Out of scope

- New features or command surface changes.
- Publishing/release process changes.

## Scope changes

- 2026-06-11: user directed that `devkey` be removed entirely rather than
  platform-gated. The workspace is now single-crate (`crates/dev`).
