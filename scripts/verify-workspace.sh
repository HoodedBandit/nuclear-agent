#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="$repo_root/target/verify-workspace"

step() {
  printf '\n==> %s\n' "$1"
  shift
  "$@"
}

optional_cargo_tool() {
  local tool="$1"
  shift
  if ! cargo --list | grep -Eq "^[[:space:]]+${tool}[[:space:]]"; then
    printf 'warning: cargo-%s is not installed; skipping. Install with: cargo install cargo-%s --locked\n' "$tool" "$tool" >&2
    return 0
  fi
  cargo "$tool" "$@"
}

run_benchmark_smoke() {
  local binary_path="$CARGO_TARGET_DIR/release/nuclear"
  local task_file="$CARGO_TARGET_DIR/benchmark-smoke.jsonl"
  local output_root="$CARGO_TARGET_DIR/benchmarks-smoke"
  local python_cmd

  if [ ! -x "$binary_path" ]; then
    printf 'benchmark smoke requires a built CLI at %s\n' "$binary_path" >&2
    return 1
  fi

  rm -rf "$output_root"
  cat >"$task_file" <<'EOF'
{"id":"repo-inspect-json","description":"Inspect the repository without a model round-trip","category":"repo_inspection","tags":["smoke","verify"],"command":["repo","inspect","--json"]}
EOF

  bash "$repo_root/scripts/run-bench.sh" "$task_file" "$binary_path" "$output_root"

  latest_run="$(find "$output_root" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
  if [ -z "$latest_run" ]; then
    printf 'benchmark smoke did not produce an output directory\n' >&2
    return 1
  fi

  if [ ! -f "$latest_run/summary.json" ] || [ ! -f "$latest_run/summary.md" ]; then
    printf 'benchmark smoke did not produce summary artifacts\n' >&2
    return 1
  fi

  if command -v python3 >/dev/null 2>&1; then
    python_cmd=python3
  elif command -v python >/dev/null 2>&1; then
    python_cmd=python
  else
    printf 'Python is required to validate benchmark artifacts\n' >&2
    return 1
  fi

  "$python_cmd" - <<'PY' "$latest_run/summary.json"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    summary = json.load(handle)

if summary.get("failed") != 0 or summary.get("passed", 0) < 1:
    raise SystemExit("benchmark smoke summary indicates failure")
PY
}

step "source LOC guard" bash "$repo_root/scripts/check-max-loc.sh"
step "cargo check --workspace" cargo check --workspace
step "cargo test --workspace" cargo test --workspace
step "cargo build --release --bin nuclear --bin autism (legacy compatibility)" cargo build --release --bin nuclear --bin autism
step "installer smoke validation" bash "$repo_root/scripts/install-smoke.sh"
step "benchmark smoke artifact validation" run_benchmark_smoke
step "cargo tree --workspace --duplicates" cargo tree --workspace --duplicates
step "cargo audit" optional_cargo_tool audit
step "cargo deny check advisories licenses bans" optional_cargo_tool deny check advisories licenses bans
step "cargo outdated -R" optional_cargo_tool outdated -R
