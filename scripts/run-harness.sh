#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if command -v python3 >/dev/null 2>&1; then
  PYTHON_CMD=python3
elif command -v python >/dev/null 2>&1; then
  PYTHON_CMD=python
else
  echo "Python is required to run the harness." >&2
  exit 1
fi

"$PYTHON_CMD" "$SCRIPT_DIR/run_harness.py" "$@"
