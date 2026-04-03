#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PACKAGE_ROOT="${1:-$REPO_ROOT/target/release/package}"
RUNTIME_CERT_ROOT="${2:-$REPO_ROOT/target/verify-ga/runtime-cert}"
CODING_DETERMINISTIC_ROOT="${3:-$REPO_ROOT/target/verify-ga/coding-deterministic}"
CODING_REFERENCE_ROOT="${4:-$REPO_ROOT/target/finalize-release/coding-reference}"
SOAK_ROOT="${5:-$REPO_ROOT/target/soak}"
OUTPUT_ROOT="${6:-$REPO_ROOT/target/release-records}"
NOTES_FILE="${7:-$REPO_ROOT/docs/ga-release-notes.md}"
CHECKLIST_FILE="${8:-$REPO_ROOT/docs/release-checklist.md}"
REQUIRE_CODING_REFERENCE="${9:-}"

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
  --runtime-cert-root "$RUNTIME_CERT_ROOT" \
  --coding-deterministic-root "$CODING_DETERMINISTIC_ROOT" \
  --coding-reference-root "$CODING_REFERENCE_ROOT" \
  --soak-root "$SOAK_ROOT" \
  --output-root "$OUTPUT_ROOT" \
  --notes-file "$NOTES_FILE" \
  --checklist-file "$CHECKLIST_FILE" \
  ${REQUIRE_CODING_REFERENCE:+--require-coding-reference}
