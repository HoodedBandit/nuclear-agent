#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PACKAGE_ROOT="${1:-$REPO_ROOT/target/phase3/package}"
BENCHMARK_SMOKE_ROOT="${2:-$REPO_ROOT/target/verify-workspace/benchmarks-smoke}"
RELEASE_EVAL_ROOT="${3:-$REPO_ROOT/target/verify-beta/release-eval}"
SOAK_ROOT="${4:-$REPO_ROOT/target/soak}"
OUTPUT_ROOT="${5:-$REPO_ROOT/target/release-records}"

if command -v python3 >/dev/null 2>&1; then
  PYTHON_BIN=python3
elif command -v python >/dev/null 2>&1; then
  PYTHON_BIN=python
else
  echo "Python is required to write release records." >&2
  exit 1
fi

"$PYTHON_BIN" "$SCRIPT_DIR/write_release_record.py" \
  --package-root "$PACKAGE_ROOT" \
  --benchmark-smoke-root "$BENCHMARK_SMOKE_ROOT" \
  --release-eval-root "$RELEASE_EVAL_ROOT" \
  --soak-root "$SOAK_ROOT" \
  --output-root "$OUTPUT_ROOT"
