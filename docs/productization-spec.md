# Devkit Productization Specification (Phase 2)

Date: 2026-06-12
Status: approved by user
Tracking: LDGR work items (`.ldgr/ldgr.db`)
Predecessor: `docs/refactor-spec.md` (completed 2026-06-12)

## Direction (user decisions, 2026-06-12)

Devkit is genuinely useful and should be positioned for adoption by others.
The accumulated personal tooling moves out:

1. **agntctl becomes its own repo immediately** (`/mnt/d/apps/agntctl`).
   The `dev agent` and `dev summary` command families were added to devkit by
   mistake; they were always meant to be a light standalone library + CLI that
   lets expensive LLMs run noisy tools (cargo build, test suites) and long
   agent tasks while receiving compact summarized reports instead of raw
   output that clutters their context.
2. **vault, research, and os families are removed entirely** — they survive
   in git history.
3. **The GPU dev container images get published to Docker Hub via CI on
   version tags** — they are genuinely useful for people setting up inference
   on aarch64 (GB10 / DGX Spark class machines).

What remains is the adoptable core: config-driven tasks and language
pipelines, env management, git/release flows, versioning, scaffolding,
setup components, docker scaffolding, review/walk.

## Work blocks

### Block P2-1 — `remove-noncore-families`
- Remove the `vault`, `research`, and `os` commands: cli.rs definitions,
  dispatch arms, `commands/{vault,research,os}.rs`, `vault.rs` support module,
  any templates and docs sections that exist only for them.
- Remove the `agent` and `summary` top-level commands (they move to agntctl):
  `commands/agent.rs`, `commands/summary.rs`, their cli/dispatch surface.
- **Keep first-class verb summarization working**: the summarized pipeline
  output for `dev fmt/lint/test/...` stays. Relocate whatever
  `commands/summary.rs` internals the verb path needs (captured execution,
  `local_summary`, summary options) into `core/summarize.rs` before deleting
  the command module.
- Update README, USAGE.md, spec.md, CLAUDE.md, and help text to the reduced
  surface in the same block — no stale docs between blocks.
- Acceptance: `dev --help` shows only core families; full verification suite
  passes; no references to removed families outside CHANGELOG/git history.

### Block P2-2 — `docker-publish-ci`
- Add `.github/workflows/docker-publish.yml`: on tags matching `v*`, build
  `build/docker/Dockerfile.core` with `docker buildx` for `linux/amd64` and
  `linux/arm64`, push to Docker Hub as
  `bakobiibizo/devkit-core:<tag>` and `:latest`.
  Uses `DOCKERHUB_USERNAME` / `DOCKERHUB_TOKEN` repo secrets (user must add
  them before the first tagged release).
- Add a README section targeting aarch64/GB10 users: what the image contains
  (NGC PyTorch base, build toolchain, uv, patched pynvml/torchaudio/
  torchvision), how to pull and use it with `dev docker init` / compose.
- Acceptance: workflow is valid (actionlint or careful review), README
  documents pull + usage; no secrets committed.

### Block P2-3 — `integration-tests`
- Add `crates/dev/tests/` integration tests using `assert_cmd` + `tempfile`
  against the built binary:
  - config generate → config check → task run round-trip in a temp project;
  - task flattening incl. cycle detection error;
  - env add/get/rm/profiles/switch on a temp `.env`;
  - git branch-create/finalize on a scratch repo (no remote, no
    release-candidate — the Block 7 acceptance scenario, automated);
  - version bump + changelog update on a temp Cargo project.
- Wire into CI test job. Target: the suite catches behavioral regressions in
  the shell-out flows that unit tests cannot.
- Acceptance: meaningful assertions (exit codes AND observable file/repo
  state), suite green in CI, runs under ~2 minutes.

### Block P2-4 — `adoption-polish`
- `Cargo.toml` crates.io metadata: description, keywords, categories,
  repository, readme, rust-version.
- README rewritten for an adopter audience: what it is in one paragraph, a
  60-second quick start, when you'd want it, link to agntctl as the companion
  tool for LLM-driven workflows.
- CHANGELOG entry and version bump to 0.4.0 (no tag/publish — user does
  release).
- Acceptance: `cargo publish --dry-run` passes; README reads as a product,
  not a personal toolbox.

## Ordering

P2-1 first (smallest surface for everything after), then P2-3 (tests guard
the now-final core), P2-2 and P2-4 last in either order.

## Out of scope

- agntctl implementation (tracked in its own repo at `/mnt/d/apps/agntctl`).
- Actually publishing to crates.io or Docker Hub (user-triggered via tags).
- Any new features.
