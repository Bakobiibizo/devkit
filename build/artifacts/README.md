# Dev Binary Artifacts

`scripts/build-dev-artifacts.sh` builds release `dev` binaries in containers that match the target runtime platform, then writes them to `.dev/bin` with SHA-256 hashes.

Default targets:

- `linux-x86_64` via Docker platform `linux/amd64`
- `linux-aarch64` via Docker platform `linux/arm64`
- `windows-x86_64` via Docker platform `linux/amd64` and Rust target `x86_64-pc-windows-gnu`

Example:

```bash
scripts/build-dev-artifacts.sh --lfs
```

Output:

```text
.dev/bin/dev
.dev/bin/dev-linux-x86_64
.dev/bin/dev-linux-aarch64
.dev/bin/dev-windows-x86_64.exe
.dev/bin/SHA256SUMS
```

The `--lfs` flag runs `git lfs install` and tracks `.dev/bin/dev*`.

macOS artifacts should be produced by native runners or platform-specific container support, then copied into `.dev/bin` using the same naming convention:

```text
.dev/bin/dev-macos-aarch64
```
