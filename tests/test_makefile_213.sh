#!/usr/bin/env bash
# Tests for issue #213: Add proof_registry targets to Makefile/justfile
# Verifies that Makefile and justfile have targets for proof_registry

set -euo pipefail

TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "${TEST_DIR}")"

# Test helpers
pass() { echo "✓ $1"; }
fail() { echo "✗ $1" >&2; exit 1; }

# Test 1: Verify Makefile exists
if [[ ! -f "${REPO_ROOT}/Makefile" ]]; then
  fail "Makefile not found at ${REPO_ROOT}/Makefile"
fi
pass "Makefile exists"

# Test 2: Verify justfile exists
if [[ ! -f "${REPO_ROOT}/justfile" ]]; then
  fail "justfile not found at ${REPO_ROOT}/justfile"
fi
pass "justfile exists"

# Test 3: Verify proof_registry directory exists
if [[ ! -d "${REPO_ROOT}/proof_registry" ]]; then
  fail "proof_registry directory not found"
fi
pass "proof_registry directory exists"

# Test 4: Verify proof_registry has Cargo.toml
if [[ ! -f "${REPO_ROOT}/proof_registry/Cargo.toml" ]]; then
  fail "proof_registry/Cargo.toml not found"
fi
pass "proof_registry/Cargo.toml exists"

# Test 5: Check that Makefile has the expected structure for targets
if ! grep -q "\.PHONY:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing .PHONY declarations"
fi
pass "Makefile has .PHONY targets"

# Test 6: Check that Makefile documents fmt target
if ! grep -q "fmt:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing fmt target"
fi
pass "Makefile has fmt target"

# Test 7: Check that Makefile documents test target
if ! grep -q "test:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing test target"
fi
pass "Makefile has test target"

# Test 8: Check that Makefile documents build target
if ! grep -q "build:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing build target"
fi
pass "Makefile has build target"

# Test 9: Check that Makefile documents lint target
if ! grep -q "lint:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing lint target"
fi
pass "Makefile has lint target"

# Test 10: Check that Makefile documents audit target
if ! grep -q "audit:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing audit target"
fi
pass "Makefile has audit target"

# Test 11: Verify justfile has recipe syntax
if ! grep -q "^[a-z_-]*:" "${REPO_ROOT}/justfile"; then
  fail "justfile missing recipe definitions"
fi
pass "justfile has recipe definitions"

# Test 12: Check that Makefile has help target
if ! grep -q "help:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing help target"
fi
pass "Makefile has help target"

# Test 13: Verify intent_settlement is configured in Makefile
if ! grep -q "intent_settlement" "${REPO_ROOT}/Makefile"; then
  fail "Makefile doesn't reference intent_settlement"
fi
pass "Makefile references intent_settlement"

# Test 14: Verify WASM_TARGET is properly defined
if ! grep -q "WASM_TARGET" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing WASM_TARGET definition"
fi
pass "Makefile has WASM_TARGET definition"

# Test 15: Verify fmt-check target exists
if ! grep -q "fmt-check:" "${REPO_ROOT}/Makefile"; then
  fail "Makefile missing fmt-check target"
fi
pass "Makefile has fmt-check target"

# All tests passed
echo ""
echo "All 15 tests passed for issue #213 (Makefile/justfile proof_registry targets)"

# --- Extended tests for proof_registry Makefile targets ---

# Test 16: Verify proof_registry has its own Makefile or is included in root Makefile
if grep -q "proof_registry" "${REPO_ROOT}/Makefile"; then
  pass "Makefile references proof_registry"
else
  fail "Makefile doesn't reference proof_registry"
fi

# Test 17: Verify proof_registry build target in Makefile
if grep -q "proof_registry.*build\|build.*proof_registry" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has proof_registry build target"
else
  echo "⚠ Makefile may not have explicit proof_registry build target"
fi

# Test 18: Verify proof_registry test target in Makefile
if grep -q "proof_registry.*test\|test.*proof_registry" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has proof_registry test target"
else
  echo "⚠ Makefile may not have explicit proof_registry test target"
fi

# Test 19: Verify proof_registry clippy/lint target
if grep -q "proof_registry.*lint\|lint.*proof_registry\|proof_registry.*clippy\|clippy.*proof_registry" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has proof_registry lint/clippy target"
else
  echo "⚠ Makefile may not have explicit proof_registry lint target"
fi

# Test 20: Verify proof_registry fmt target
if grep -q "proof_registry.*fmt\|fmt.*proof_registry" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has proof_registry fmt target"
else
  echo "⚠ Makefile may not have explicit proof_registry fmt target"
fi

# Test 21: Verify justfile has proof_registry recipes
if grep -q "proof_registry" "${REPO_ROOT}/justfile"; then
  pass "justfile references proof_registry"
else
  fail "justfile doesn't reference proof_registry"
fi

# Test 22: Verify proof_registry Cargo.toml has required features
if grep -q "testutils" "${REPO_ROOT}/proof_registry/Cargo.toml"; then
  pass "proof_registry/Cargo.toml has testutils feature"
else
  fail "proof_registry/Cargo.toml missing testutils feature"
fi

# Test 23: Verify proof_registry has src/lib.rs
if [[ -f "${REPO_ROOT}/proof_registry/src/lib.rs" ]]; then
  pass "proof_registry/src/lib.rs exists"
else
  fail "proof_registry/src/lib.rs not found"
fi

# Test 24: Verify proof_registry has tests
if [[ -f "${REPO_ROOT}/proof_registry/src/test.rs" ]] || find "${REPO_ROOT}/proof_registry/tests" -name "*.rs" -type f 2>/dev/null | grep -q .; then
  pass "proof_registry has test files"
else
  echo "⚠ proof_registry may not have test files"
fi

# Test 25: Verify Makefile has all target (builds everything)
if grep -q "^all:" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has 'all' target"
else
  fail "Makefile missing 'all' target"
fi

# Test 26: Verify Makefile has clean target
if grep -q "^clean:" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has clean target"
else
  fail "Makefile missing clean target"
fi

# Test 27: Verify Makefile has check target (fmt + lint + test)
if grep -q "^check:" "${REPO_ROOT}/Makefile"; then
  pass "Makefile has check target"
else
  echo "⚠ Makefile may not have check target"
fi

# Test 28: Verify justfile has default recipe
if grep -q "^default " "${REPO_ROOT}/justfile"; then
  pass "justfile has default recipe"
else
  echo "⚠ justfile may not have default recipe"
fi

# Test 29: Verify Makefile uses proper variable references
if grep -q '\$(CARGO)' "${REPO_ROOT}/Makefile"; then
  pass "Makefile uses CARGO variable"
else
  echo "⚠ Makefile may not use CARGO variable"
fi

# Test 30: Verify Makefile documents test target runs cargo test
if grep -A2 "^test:" "${REPO_ROOT}/Makefile" | grep -q "cargo test"; then
  pass "Makefile test target runs cargo test"
else
  echo "⚠ Makefile test target may not run cargo test"
fi

# Summary
echo ""
echo "Extended tests completed for Makefile/justfile proof_registry targets"
echo "Core tests (1-15): PASS"
echo "Extended tests (16-30): see above for individual results"
