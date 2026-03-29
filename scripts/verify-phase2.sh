#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary_path="${1:-}"

if [ -z "$binary_path" ]; then
  for candidate in \
    "$repo_root/target/verify-workspace/release/nuclear" \
    "$repo_root/target/release/nuclear" \
    "$repo_root/target/debug/nuclear"
  do
    if [ -x "$candidate" ]; then
      binary_path="$candidate"
      break
    fi
  done
fi

if [ -z "$binary_path" ]; then
  printf 'Could not find a built nuclear binary. Run verify-workspace.sh first or pass the binary path.\n' >&2
  exit 1
fi

if command -v python3 >/dev/null 2>&1; then
  python_cmd=python3
elif command -v python >/dev/null 2>&1; then
  python_cmd=python
else
  printf 'Python is required to run the Phase 2 smoke verification.\n' >&2
  exit 1
fi

"$python_cmd" -u "$repo_root/scripts/phase2_smoke.py" \
  --binary-path "$binary_path" \
  --repo-root "$repo_root" \
  --scenario-root "$repo_root/target/phase2-smoke/linux"

"$python_cmd" -u "$repo_root/scripts/phase2_matrix.py" \
  --binary-path "$binary_path" \
  --repo-root "$repo_root" \
  --scenario-root "$repo_root/target/phase2-matrix/linux"
