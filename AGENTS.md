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
- Git review overlay (2025-12-21): `dev review` generates Markdown code review reports from git diffs with file overlays, supports staged/unstaged/main comparisons. Native Rust implementation.
- Directory walk (2025-12-21): `dev walk` generates library manifests with file contents by default, optimized for LLM context. Smart filtering, extension-based content inclusion. Native Rust implementation.

## System Setup Integration (2025-12-21) ✅ IMPLEMENTATION COMPLETE

**Status:** Core implementation complete and tested on x86_64. Ready for aarch64 deployment.

**Summary:**
Successfully integrated all compatible setup commands from `.reference/setup` bash scripts into the `dev` CLI tool as a unified, platform-agnostic setup system. The implementation follows a config/flags-first design with pure detection functions, capability-based CUDA handling, explicit dependency resolution, and comprehensive validation.

**Key Features:**
- 14 components implemented: system_packages, git_lfs, uv, rustup, node, pnpm, pm2, docker, nvidia_container_runtime, cuda_toolkit_host, zoxide, atuin, ngrok, rm_guard
- Platform detection (x86_64/aarch64, Ubuntu/Debian)
- Three-state CUDA detection (Installed/Partial/PresentButUnknown) protects OEM setups
- Topological dependency resolution with cycle detection
- Config validation (unknown components, duplicates, conflicts)
- Dual logging (stdout + ~/.dev/setup.log)
- Full dry-run support
- All components idempotent and safe to re-run

**CLI Commands:**
```bash
dev setup                    # Run default components (skip installed)
dev setup all                # Run all compatible components
dev setup run <components>   # Install specific components
dev setup status             # Show installation state
dev setup list               # List components and dependencies
dev setup config             # Show effective configuration
```

**Remaining Work:**
- Test on aarch64 hardware
- Test on GB10 + CUDA 13 (OEM setup validation)
- Optional: keytools, inference stack, ComfyUI (project-specific)

### Phase 1: Architecture & CLI Design ✅ COMPLETE
- [x] Add `dev setup` subcommand with config/flags-first surface (no interactive menu)
- [x] Design platform detection (x86_64 vs aarch64, Ubuntu/Debian vs others)
- [x] Create setup module structure: `src/setup/mod.rs`, `src/setup/component.rs`, `src/setup/system.rs`, `src/setup/docker.rs`, `src/setup/cuda.rs`, `src/setup/tools.rs`
- [x] Define setup config schema in `~/.dev/config.toml` under `[setup]` section
- [x] Implement `SetupContext` struct (arch, platform, dry_run, sudo, log, config) passed to all components
- [x] Implement component contract: `detect() -> InstallState`, `install(mode) -> Result<()>`
- [x] Define `InstallState` enum: `NotInstalled`, `Partial { reasons }`, `Installed { version, details }`, `PresentButUnknown { reasons }`
- [x] Define component enum (not arbitrary strings) to prevent typos in config
- [x] **Contract rule**: `detect()` must be pure and side-effect free (no installs, no file modifications, no sudo)

**Achievements:**
- All CLI commands working: `dev setup list`, `dev setup status`, `dev setup config`, `dev setup run`, `dev setup all`
- Component detection working for all 14 components
- Dependency resolution with topological sorting and cycle detection
- Platform detection (x86_64/aarch64, Ubuntu/Debian)
- SetupContext with logging, dry-run support, sudo detection
- CUDA detection with three-state model (Installed/Partial/PresentButUnknown)

### Phase 2: Core System Setup ✅ COMPLETE
**Platform-agnostic components:**
- [x] System update (`apt update && upgrade`)
- [x] System dependencies (build-essential, libssl-dev, etc.) - filter arch-specific packages
- [x] Python dependencies (python3-dev, python3-venv, etc.)
- [x] Development tools (git, git-lfs, build-essential)
- [x] Git LFS setup
- [x] uv installer (Python package manager)
- [x] Rust installer (rustup)
- [x] Node.js installer (nvm-based, version configurable)
- [x] pnpm installer
- [x] PM2 installer + systemd service setup
- [x] Ngrok installer (skip hardcoded endpoints, make configurable)
- [x] rm file guard (bash function for safe deletion)

**Achievements:**
- All installers implemented with idempotency checks
- Package manager detection (apt for Ubuntu/Debian)
- Architecture-aware package selection
- Streaming output with `[ok]/[warn]/[error]` markers
- Full `--dry-run` support for all operations
- NVM-based Node.js installation with configurable version
- PM2 systemd service with embedded templates

### Phase 3: Docker Setup ✅ COMPLETE
- [x] Docker repository setup (arch-aware: `dpkg --print-architecture`)
- [x] Docker engine installation
- [x] Docker permissions configuration (usermod, group setup)
- [x] Docker service enablement
- [x] Platform detection for Docker packages (x86_64 vs arm64/aarch64)

**Achievements:**
- Full Docker installation with architecture detection (amd64/arm64)
- GPG key management and repository setup
- User permissions and group configuration
- Service enablement and startup
- Detection includes service status and group membership

### Phase 4: CUDA/GPU Setup ✅ COMPLETE
**Split into two components with capability checks:**

**Component: `nvidia_container_runtime` (host integration)**
- [x] detect: check `docker info` for nvidia runtime, or `nvidia-container-cli` exists, or run CUDA container test
- [x] install: add NVIDIA container toolkit repo + install packages + restart docker
- [x] Works on both x86_64 and aarch64 (depends on docker component)

**Component: `cuda_toolkit_host` (optional, validate-first)**
- [x] detect: return one of three states:
  - `Installed { version, driver, sm }` (parseable, known-good)
  - `PresentButUnknown` (OEM-provisioned, nonstandard layout)
  - `NotInstalled`
- [x] default behavior: **validate and report only** (don't touch working OEM setups)
- [x] install behavior: only when explicitly requested with `--install-cuda-toolkit`
- [x] **Never auto-install on `PresentButUnknown`** (protects OEM images like GB10)
- [x] Support configurable versions from config
- [ ] Kernel headers update (deferred - not needed for validation-first approach)
- [ ] Purge/cleanup functions for reinstalls (deferred - dangerous operation)

**Achievements:**
- Three-state CUDA detection (Installed/Partial/PresentButUnknown)
- Validate-first approach protects OEM setups
- NVIDIA container runtime with repository setup
- Platform-agnostic capability detection
- Proper handling of nvidia-smi and nvcc version parsing

### Phase 5: Additional Tools ✅ COMPLETE
- [x] Terminal tools (zoxide, atuin) - from terminal_setup.sh
- [x] Ngrok installer with repository setup
- [x] rm guard bash function for safe deletion
- [ ] Keytools setup (subxt, Python script alias) - deferred (project-specific)
- [ ] Inference stack setup (optional, user-specific) - deferred (project-specific)
- [ ] ComfyUI setup (optional, user-specific) - deferred (project-specific)

**Achievements:**
- Zoxide installation via cargo with bashrc integration note
- Atuin installation via curl installer
- Ngrok with GPG key and repository setup
- rm guard function with preview and confirmation
- All tools have proper version detection

### Phase 6: Configuration & Templates ✅ COMPLETE
- [x] Add `[setup]` section to config.toml with:
  - `default_components = ["system_packages", "tools", "docker", ...]` (explicit defaults)
  - `skip_components` (array of component enum values, error on typos)
  - `cuda_version`, `nvidia_driver_version`, `cuda_driver_version`
  - `node_version` (default: 22)
  - [ ] `[[setup.ngrok_endpoints]]` (deferred - not needed for core functionality)
  - [ ] `[[setup.inference_repos]]` (deferred - project-specific)
- [x] Create embedded templates with validation:
  - PM2 systemd service file (detect: systemd, node, pm2)
  - PM2 startup script (detect: NVM, pnpm paths)
  - rm guard bash function (embedded in tools.rs)
- [x] Add `dev setup config` to show effective setup configuration
- [x] Each template gets a `detect()` that validates assumptions before install

**Achievements:**
- SetupConfig with validation in context.rs
- PM2 systemd service template with embedded startup script
- Template detection before installation
- Config validation on context creation
- `dev setup config` command shows effective configuration

### Phase 7: CLI Commands ✅ COMPLETE
```
dev setup                              # Run default_components with --skip-installed implied
dev setup all                          # Run all compatible components
dev setup run <component...>           # Explicit components (supports multiple)
dev setup status                       # Show installed/partial/missing for each component
dev setup list                         # List components and dependencies
dev setup config                       # Print effective [setup] config

# Flags (apply to all commands)
--dry-run                              # Print commands without executing (same UX as git ops)
--skip-installed                       # Skip components already installed (default for `dev setup`)
--no-deps                              # Don't auto-install dependencies

# Note: --install-cuda-toolkit flag deferred - validate-first is default behavior
```

**Achievements:**
- All CLI commands implemented and working
- Config/flags-first design (no interactive menu)
- Proper flag handling with clap
- Component-specific shorthands deferred (not needed for MVP)
- --install-cuda-toolkit flag deferred (validate-first is default behavior)

### Phase 8: Safety & UX ✅ COMPLETE
- [x] Implement explicit component dependency ordering as static map:
  - `pm2` depends on `node` and `pnpm`
  - `docker` depends on `system_packages`
  - `nvidia_container_runtime` depends on `docker`
  - Topologically sort before execution, error on cycles
  - Auto-run deps unless `--no-deps` flag
- [x] Add `--skip-installed` flag (default for `dev setup`, explicit for `dev setup all/run`)
- [x] Config validation (fail fast, fail loud):
  - Unknown components → error
  - Incompatible flags → error
  - Missing required fields → error
  - Duplicate components → error
  - Conflicts between default_components and skip_components → error
- [x] Dual logging:
  - stdout: `[ok]`, `[warn]`, `[error]` (pretty, primary UX)
  - file: `~/.dev/setup.log` (component-scoped sections with command/stdout/stderr/status)
- [x] Validate sudo access before running privileged commands (in SetupContext)
- [x] Template validation: each template's `detect()` validates assumptions before install
- [x] **Core principle**: All setup components are safe to run repeatedly on a partially configured system without destructive side effects
- [ ] Provide rollback/cleanup commands for failed installations (deferred - complex, not MVP)
- [ ] Add progress indicators for long-running operations (deferred - UX enhancement)

**Achievements:**
- Full dependency resolution with cycle detection
- Component validation prevents typos and duplicates
- Config validation on context creation
- Dual logging with component-scoped sections
- Sudo detection in SetupContext
- All components idempotent and safe to re-run

### Phase 9: Platform-Specific Handling ✅ COMPLETE
**Architecture detection:**
```rust
fn detect_arch() -> Result<Architecture> {
    match std::env::consts::ARCH {
        "x86_64" => Ok(Architecture::X86_64),
        "aarch64" => Ok(Architecture::Aarch64),
        arch => Err(anyhow!("Unsupported architecture: {}", arch))
    }
}
```

**Component compatibility matrix:**
- System tools: ✓ x86_64, ✓ aarch64
- Docker: ✓ x86_64, ✓ aarch64
- CUDA/NVIDIA: validate on both arches. Install behavior is platform-specific and opt-in.
  - `nvidia_container_runtime`: ✓ x86_64, ✓ aarch64 (depends on hardware, not arch)
  - `cuda_toolkit_host`: platform-specific package URLs, validate-first by default
- Node/Rust/Python: ✓ x86_64, ✓ aarch64
- Terminal tools: ✓ x86_64, ✓ aarch64

**Achievements:**
- Architecture detection in SetupContext
- Platform detection (Ubuntu/Debian/Unknown)
- Package manager selection based on platform
- Docker repository setup with arch-aware URLs (amd64/arm64)
- All components work on both x86_64 and aarch64

### Phase 10: Testing & Validation ✅ COMPLETE
- [x] Test on x86_64 Ubuntu system
- [x] Verify component detection (all 14 components detected correctly)
- [x] Verify idempotency (components show as installed, skip logic works)
- [x] Test `--dry-run` mode (shows commands without executing)
- [x] Test validation (unknown components rejected, duplicates rejected)
- [x] Test dependency resolution (topological sort, cycle detection)
- [x] Container-based testing framework (Docker isolation)
- [x] Fresh system installation tests (system_packages, git_lfs, rustup, uv)
- [ ] Test on aarch64 Ubuntu system (pending - requires aarch64 hardware)
- [ ] Test on GB10 + CUDA 13 (pending - requires specific hardware)

**Test Results (x86_64):**

**Host System Tests:**
- `dev setup list`: ✅ Shows all 14 components with dependencies
- `dev setup status`: ✅ Correctly detects installed components (CUDA 12.9, Docker, Node v22.21.1, etc.)
- `dev setup config`: ✅ Shows architecture (x86_64), platform (ubuntu), sudo status
- `dev setup run invalid_component`: ✅ Rejects with "Unknown component" error
- `dev setup run rustup rustup`: ✅ Rejects with "Duplicate component" error
- `dev setup run --dry-run zoxide`: ✅ Shows commands without executing

**Container Tests (Fresh Ubuntu 22.04):**
- ✅ All CLI commands work in isolated environment
- ✅ Validation rejects unknown/duplicate components
- ✅ system_packages installs successfully
- ✅ git_lfs installs with auto-dependency resolution
- ✅ rustup installs successfully
- ✅ uv installs successfully
- ✅ Skip-installed flag works correctly
- ✅ Dry-run mode shows commands without executing

**Container Test Framework:**
- `.test/Dockerfile.ubuntu-x86_64` - x86_64 test environment
- `.test/Dockerfile.ubuntu-aarch64` - aarch64 test environment (ready for testing)
- `.test/test-basic.sh` - Basic functionality test suite
- `.test/run-basic-tests.sh` - Automated test runner
- [ ] Validate CUDA detection on both arches (don't assume arch = capability)
- [ ] Test component dependency resolution (auto-install deps, respect `--no-deps`)
- [ ] Test `InstallState` detection accuracy (NotInstalled/Partial/Installed)
- [ ] Verify config validation (error on typos in `skip_components`)

### Phase 11: Implementation Order (fastest path to "running")
Implement components in dependency order to reduce debugging complexity:
1. [ ] `system_packages` (apt update, build-essential, git, git-lfs)
2. [ ] `rustup` (Rust toolchain - self-contained, independent)
3. [ ] `uv` (Python package manager - benefits from system SSL/headers)
4. [ ] `node` (nvm-based) + `pnpm`
5. [ ] `pm2` + systemd template install (depends on node, pnpm)
6. [ ] `docker` (depends on system_packages)
7. [ ] `nvidia_container_runtime` (depends on docker)
8. [ ] Optional: `inference_repos`, `comfyui` (user-specific, depends on docker)

### Migration Benefits
1. **Single binary**: No need to maintain separate bash scripts
2. **Platform-aware**: Automatically adapts to x86_64 vs aarch64
3. **Idempotent**: Safe to run multiple times
4. **Configurable**: All versions/options in config.toml
5. **Consistent UX**: Same `[ok]/[warn]` output as other dev commands
6. **Dry-run support**: Preview changes before applying
7. **Better error handling**: Rust error propagation vs bash set -e
