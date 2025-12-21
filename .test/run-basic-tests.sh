#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Building dev binary ==="
cd "$PROJECT_ROOT"
cargo build --release

echo ""
echo "=== Building test container (x86_64) ==="
docker build -f .test/Dockerfile.ubuntu-x86_64 -t dev-test:x86_64 .

echo ""
echo "=== Running basic tests in container ==="
docker run --rm \
    -v "$PROJECT_ROOT/.test/test-basic.sh:/home/testuser/test-basic.sh:ro" \
    dev-test:x86_64 \
    bash /home/testuser/test-basic.sh

echo ""
echo "=== All container tests passed! ==="
