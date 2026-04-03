#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary_path="${1:-$repo_root/target/verify-workspace/release/nuclear}"
output_root="${2:-${TMPDIR:-/tmp}/nuclear-support-bundle-smoke-$$}"

if command -v python3 >/dev/null 2>&1; then
  python_cmd=python3
elif command -v python >/dev/null 2>&1; then
  python_cmd=python
else
  printf 'Python is required to run the support bundle smoke test.\n' >&2
  exit 1
fi

"$python_cmd" "$repo_root/scripts/support_bundle_smoke.py" \
  --binary-path "$binary_path" \
  --repo-root "$repo_root" \
  --scenario-root "$output_root"
