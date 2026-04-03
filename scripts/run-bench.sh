#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TASK_FILE="${1:-./benchmarks/coding-smoke/tasks.jsonl}"
BINARY_PATH="${2:-}"
OUTPUT_ROOT="${3:-}"

ARGS=(--lane analysis-smoke --task-file "$TASK_FILE")
if [ -n "$BINARY_PATH" ]; then
  ARGS+=(--binary-path "$BINARY_PATH")
fi
if [ -n "$OUTPUT_ROOT" ]; then
  ARGS+=(--output-root "$OUTPUT_ROOT")
fi

bash "$SCRIPT_DIR/run-harness.sh" "${ARGS[@]}"
