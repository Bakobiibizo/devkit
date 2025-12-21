#!/bin/bash
set -e

echo "=== Testing dev setup in container ==="
echo ""

# Test 1: List components
echo "Test 1: dev setup list"
dev setup list
echo ""

# Test 2: Show status (should show nothing installed)
echo "Test 2: dev setup status (before installation)"
dev setup status
echo ""

# Test 3: Show config
echo "Test 3: dev setup config"
dev setup config
echo ""

# Test 4: Dry-run installation
echo "Test 4: dev setup run --dry-run system_packages"
dev setup run --dry-run system_packages
echo ""

# Test 5: Test validation - unknown component
echo "Test 5: Validation test (should fail)"
if dev setup run invalid_component 2>&1 | grep -q "Unknown component"; then
    echo "✓ Validation working: unknown component rejected"
else
    echo "✗ Validation failed: unknown component not rejected"
    exit 1
fi
echo ""

# Test 6: Test validation - duplicate component
echo "Test 6: Duplicate test (should fail)"
if dev setup run rustup rustup 2>&1 | grep -q "Duplicate component"; then
    echo "✓ Validation working: duplicate component rejected"
else
    echo "✗ Validation failed: duplicate component not rejected"
    exit 1
fi
echo ""

# Test 7: Install system_packages (requires sudo)
echo "Test 7: dev setup run system_packages"
dev setup run system_packages
echo ""

# Test 8: Verify installation
echo "Test 8: dev setup status (after system_packages)"
dev setup status
echo ""

# Test 9: Test skip-installed flag
echo "Test 9: dev setup run --skip-installed system_packages (should skip)"
dev setup run --skip-installed system_packages
echo ""

# Test 10: Install git_lfs (depends on system_packages)
echo "Test 10: dev setup run git_lfs"
dev setup run git_lfs
echo ""

# Test 11: Test dependency resolution
echo "Test 11: dev setup run pnpm (should auto-install node)"
dev setup run pnpm
echo ""

# Test 12: Verify all installations
echo "Test 12: Final status check"
dev setup status
echo ""

echo "=== All tests passed! ==="
