#!/usr/bin/env bash
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TASK_FILE="${1:-$REPO_ROOT/benchmarks/coding-smoke/tasks.jsonl}"
BINARY_PATH="${2:-}"
OUTPUT_ROOT="${3:-$REPO_ROOT/target/benchmarks}"

if command -v python3 >/dev/null 2>&1; then
  PYTHON_CMD=python3
elif command -v python >/dev/null 2>&1; then
  PYTHON_CMD=python
else
  echo "Python is required to run benchmarks." >&2
  exit 1
fi

ARGS=("$SCRIPT_DIR/run_bench.py" "--task-file" "$TASK_FILE" "--output-root" "$OUTPUT_ROOT")
if [ -n "$BINARY_PATH" ]; then
  ARGS+=("--binary-path" "$BINARY_PATH")
fi

"$PYTHON_CMD" "${ARGS[@]}"
