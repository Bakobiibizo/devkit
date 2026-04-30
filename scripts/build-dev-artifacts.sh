#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-${ROOT}/.dev/bin}"
DOCKERFILE="${ROOT}/build/artifacts/Dockerfile.dev"
IMAGE_PREFIX="${IMAGE_PREFIX:-devkit-dev-artifact}"
PACKAGE="${PACKAGE:-devkit-cli}"
BIN="${BIN:-dev}"
SETUP_LFS=0
TARGETS=()
SUPPORTED_TARGETS=(linux-x86_64 linux-aarch64 windows-x86_64)

usage() {
  cat <<'EOF'
Build dev release artifacts inside platform-matched containers.

Usage:
  scripts/build-dev-artifacts.sh [options] [target...]

Targets:
  linux-x86_64       Build with docker --platform linux/amd64
  linux-aarch64      Build with docker --platform linux/arm64
  windows-x86_64     Cross-build x86_64-pc-windows-gnu in linux/amd64

Options:
  -o, --out-dir DIR  Output directory (default: .dev/bin)
      --lfs          Run git lfs install/track for .dev/bin/dev*
  -h, --help         Show this help

Environment:
  OUT_DIR            Output directory
  IMAGE_PREFIX       Docker image prefix
  PACKAGE            Cargo package to build (default: devkit-cli)
  BIN                Binary name to build (default: dev)

Examples:
  scripts/build-dev-artifacts.sh
  scripts/build-dev-artifacts.sh linux-x86_64 linux-aarch64 --lfs
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -o|--out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --lfs)
      SETUP_LFS=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    linux-x86_64|linux-aarch64|windows-x86_64)
      TARGETS+=("$1")
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ${#TARGETS[@]} -eq 0 ]]; then
  TARGETS=("${SUPPORTED_TARGETS[@]}")
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required to build dev artifacts" >&2
  exit 127
fi

mkdir -p "${OUT_DIR}"
rm -f \
  "${OUT_DIR}/dev" \
  "${OUT_DIR}/dev-linux-x86_64" \
  "${OUT_DIR}/dev-linux-aarch64" \
  "${OUT_DIR}/dev-windows-x86_64.exe" \
  "${OUT_DIR}/SHA256SUMS"

platform_for_target() {
  case "$1" in
    linux-x86_64) echo "linux/amd64" ;;
    linux-aarch64) echo "linux/arm64" ;;
    windows-x86_64) echo "linux/amd64" ;;
    *) echo "unsupported target: $1" >&2; exit 2 ;;
  esac
}

artifact_for_target() {
  case "$1" in
    linux-x86_64) echo "dev-linux-x86_64" ;;
    linux-aarch64) echo "dev-linux-aarch64" ;;
    windows-x86_64) echo "dev-windows-x86_64.exe" ;;
    *) echo "unsupported target: $1" >&2; exit 2 ;;
  esac
}

rust_target_for_target() {
  case "$1" in
    windows-x86_64) echo "x86_64-pc-windows-gnu" ;;
    *) echo "" ;;
  esac
}

build_target() {
  local target="$1"
  local platform artifact image container rust_target out_name
  platform="$(platform_for_target "${target}")"
  artifact="$(artifact_for_target "${target}")"
  rust_target="$(rust_target_for_target "${target}")"
  out_name="${BIN}"
  if [[ "${artifact}" == *.exe ]]; then
    out_name="${BIN}.exe"
  fi
  image="${IMAGE_PREFIX}:${target}"

  echo "[build] ${target} (${platform})"
  docker build \
    --platform "${platform}" \
    --file "${DOCKERFILE}" \
    --build-arg "PACKAGE=${PACKAGE}" \
    --build-arg "BIN=${BIN}" \
    --build-arg "TARGET=${rust_target}" \
    --tag "${image}" \
    "${ROOT}"

  container="$(docker create "${image}")"
  trap 'docker rm -f "${container}" >/dev/null 2>&1 || true' RETURN
  docker cp "${container}:/out/${out_name}" "${OUT_DIR}/${artifact}"
  docker rm "${container}" >/dev/null
  trap - RETURN
  chmod +x "${OUT_DIR}/${artifact}"
}

for target in "${TARGETS[@]}"; do
  build_target "${target}"
done

if [[ -f "${OUT_DIR}/dev-linux-x86_64" ]]; then
  cp "${OUT_DIR}/dev-linux-x86_64" "${OUT_DIR}/dev"
  chmod +x "${OUT_DIR}/dev"
fi

(
  cd "${OUT_DIR}"
  shopt -s nullglob
  artifacts=(dev*)
  if [[ ${#artifacts[@]} -eq 0 ]]; then
    echo "no dev artifacts were produced" >&2
    exit 1
  fi
  sha256sum "${artifacts[@]}" > SHA256SUMS
)

if [[ "${SETUP_LFS}" -eq 1 ]]; then
  if ! command -v git >/dev/null 2>&1 || ! git -C "${ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "[warn] git repository not detected; skipped Git LFS tracking" >&2
  elif ! git lfs version >/dev/null 2>&1; then
    echo "[warn] git-lfs is not installed; skipped Git LFS tracking" >&2
  else
    git -C "${ROOT}" lfs install
    git -C "${ROOT}" lfs track ".dev/bin/dev*"
  fi
fi

echo "[ok] wrote artifacts to ${OUT_DIR}"
echo "[ok] wrote ${OUT_DIR}/SHA256SUMS"
