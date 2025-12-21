# Setup System Testing

This directory contains Docker-based testing infrastructure for the `dev setup` system.

## Test Environments

### x86_64 Ubuntu 22.04
```bash
./run-basic-tests.sh
```

Runs basic functionality tests in a fresh Ubuntu 22.04 container:
- Component listing and status
- Configuration display
- Validation (unknown/duplicate components)
- Dry-run mode
- Installation (system_packages, git_lfs, rustup, uv)
- Idempotency (skip-installed flag)
- Dependency resolution

### aarch64 Ubuntu 22.04
```bash
# Build aarch64 binary (requires cross-compilation or aarch64 host)
cargo build --release --target aarch64-unknown-linux-gnu

# Build and run aarch64 container
docker build -f Dockerfile.ubuntu-aarch64 -t dev-test:aarch64 .
docker run --rm --privileged \
    -v "$(pwd)/test-basic.sh:/home/testuser/test-basic.sh:ro" \
    dev-test:aarch64 \
    bash /home/testuser/test-basic.sh
```

## Test Structure

- `Dockerfile.ubuntu-x86_64` - x86_64 test container definition
- `Dockerfile.ubuntu-aarch64` - aarch64 test container definition
- `test-basic.sh` - Core functionality test suite
- `test-setup.sh` - Extended test suite (includes Node.js/pnpm)
- `run-basic-tests.sh` - Automated test runner for x86_64

## What Gets Tested

### Validation
- Unknown component names → error
- Duplicate components → error
- Empty component list → error

### CLI Commands
- `dev setup list` - component listing with dependencies
- `dev setup status` - installation state detection
- `dev setup config` - configuration display
- `dev setup run <components>` - component installation
- `dev setup run --dry-run` - command preview
- `dev setup run --skip-installed` - idempotency

### Installation
- system_packages (apt update, build-essential, etc.)
- git_lfs (with dependency auto-resolution)
- rustup (Rust toolchain)
- uv (Python package manager)

### Dependency Resolution
- Topological sorting
- Auto-installation of dependencies
- Cycle detection

## Test Results

All tests pass on x86_64 Ubuntu 22.04:
- ✅ Validation working
- ✅ All CLI commands functional
- ✅ Installations succeed
- ✅ Idempotency verified
- ✅ Dependency resolution working

## Notes

- Tests run in containers with passwordless sudo configured for testuser
- Fresh Ubuntu 22.04 base ensures clean environment
- Binary is copied into container at build time
- Tests are read-only mounted at runtime
- aarch64 Dockerfile accepts DEV_BIN build arg for cross-compiled binary path
