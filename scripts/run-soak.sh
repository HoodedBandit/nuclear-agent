#!/usr/bin/env bash
set -euo pipefail

TOKEN="${1:-${AGENT_TOKEN:-}}"
BASE_URL="${2:-${AGENT_BASE_URL:-http://127.0.0.1:42690}}"
ITERATIONS="${3:-30}"
DELAY_MS="${4:-1000}"
WORKSPACE="${5:-${AGENT_WORKSPACE_PATH:-}}"
OUTPUT_ROOT="${6:-${AGENT_SOAK_OUTPUT_ROOT:-}}"

if [[ -z "${TOKEN}" ]]; then
  echo "usage: ./scripts/run-soak.sh <token> [base-url] [iterations] [delay-ms] [workspace] [output-root]" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARGS=(
  --token "${TOKEN}"
  --base-url "${BASE_URL}"
  --iterations "${ITERATIONS}"
  --delay-ms "${DELAY_MS}"
)

if [[ -n "${WORKSPACE}" ]]; then
  ARGS+=(--workspace "${WORKSPACE}")
fi

if [[ -n "${OUTPUT_ROOT}" ]]; then
  ARGS+=(--output-root "${OUTPUT_ROOT}")
fi

node "${SCRIPT_DIR}/run-soak.cjs" "${ARGS[@]}"
