#!/usr/bin/env sh
set -eu

# Install the devkit `dev` binary from GitHub releases.
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/bakobiibizo/devkit/main/scripts/install.sh | sh
# Optional environment:
#   DEVKIT_REPO=bakobiibizo/devkit
#   DEVKIT_VERSION=v0.4.0        # default: latest
#   DEVKIT_INSTALL_DIR=$HOME/.local/bin

repo="${DEVKIT_REPO:-bakobiibizo/devkit}"
version="${DEVKIT_VERSION:-latest}"
install_dir="${DEVKIT_INSTALL_DIR:-$HOME/.local/bin}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' was not found" >&2
    exit 1
  fi
}

need curl
need tar
need uname

case "$(uname -s)" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "error: unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

target="$arch-$os"

if [ "$version" = "latest" ]; then
  version="$(curl -fsSL -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/$repo/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1)"
  if [ -z "$version" ]; then
    echo "error: failed to resolve latest release for $repo" >&2
    exit 1
  fi
fi

case "$version" in
  v*) ;;
  *) version="v$version" ;;
esac

asset="dev-$version-$target.tar.gz"
base_url="https://github.com/$repo/releases/download/$version"
tmp="$(mktemp -d 2>/dev/null || mktemp -d -t devkit-install)"
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT INT TERM

echo "Installing dev $version for $target"
echo "Destination: $install_dir/dev"

curl -fL --proto '=https' --tlsv1.2 -o "$tmp/$asset" "$base_url/$asset"

if command -v sha256sum >/dev/null 2>&1; then
  if curl -fsSL -o "$tmp/checksums.txt" "$base_url/checksums.txt"; then
    expected="$(grep "  $asset\$" "$tmp/checksums.txt" | awk '{print $1}')"
    if [ -n "$expected" ]; then
      actual="$(sha256sum "$tmp/$asset" | awk '{print $1}')"
      if [ "$expected" != "$actual" ]; then
        echo "error: checksum mismatch for $asset" >&2
        exit 1
      fi
      echo "Verified checksum"
    else
      echo "warning: checksums.txt did not contain $asset; skipping checksum verification" >&2
    fi
  else
    echo "warning: checksums.txt not found; skipping checksum verification" >&2
  fi
else
  echo "warning: sha256sum not found; skipping checksum verification" >&2
fi

tar -xzf "$tmp/$asset" -C "$tmp"
if [ ! -f "$tmp/dev" ]; then
  echo "error: archive did not contain dev binary at root" >&2
  exit 1
fi

mkdir -p "$install_dir"
if [ -f "$install_dir/dev" ]; then
  mv "$install_dir/dev" "$install_dir/dev.old"
fi

install -m 755 "$tmp/dev" "$install_dir/dev"
"$install_dir/dev" --version

echo "dev installed successfully. Ensure $install_dir is on PATH."
