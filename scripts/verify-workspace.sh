#!/usr/bin/env bash
set -euo pipefail

if [ -d "${HOME:-}/.cargo/bin" ] && ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

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
  local cargo_home="${CARGO_HOME:-$HOME/.cargo}"
  local tool_path="$cargo_home/bin/cargo-$tool"
  if [ ! -x "$tool_path" ] && ! command -v "cargo-$tool" >/dev/null 2>&1; then
    if [ "${CI:-}" = "true" ]; then
      printf 'cargo-%s is not installed. Install with: cargo install cargo-%s --locked\n' "$tool" "$tool" >&2
      return 1
    fi
    printf 'warning: cargo-%s is not installed; skipping local check. Install with: cargo install cargo-%s --locked\n' "$tool" "$tool" >&2
    return 0
  fi
  cargo "$tool" "$@"
}

run_workspace_dependency_drift_check() {
  local python_cmd
  if command -v python3 >/dev/null 2>&1; then
    python_cmd=python3
  elif command -v python >/dev/null 2>&1; then
    python_cmd=python
  else
    printf 'Python is required for the workspace dependency drift check\n' >&2
    return 1
  fi

  "$python_cmd" "$repo_root/scripts/check-workspace-dependency-drift.py"
}

run_release_gate_script_tests() {
  local python_cmd

  if command -v python3 >/dev/null 2>&1; then
    python_cmd=python3
  elif command -v python >/dev/null 2>&1; then
    python_cmd=python
  else
    printf 'Python is required for the release gate script tests\n' >&2
    return 1
  fi

  "$python_cmd" -m unittest discover -s "$repo_root/scripts/tests" -p "test_*.py"
}

run_dashboard_checks() {
  local dashboard_root="$repo_root/ui/dashboard"
  if [ ! -d "$dashboard_root" ]; then
    return 0
  fi

  if [ ! -f "$dashboard_root/package-lock.json" ]; then
    printf 'dashboard checks require %s/package-lock.json\n' "$dashboard_root" >&2
    return 1
  fi

  npm --prefix "$dashboard_root" ci
  npm --prefix "$dashboard_root" run typecheck
  npm --prefix "$dashboard_root" run lint
  npm --prefix "$dashboard_root" test
  npm --prefix "$dashboard_root" run build
}

run_runtime_smoke() {
  local binary_path="$CARGO_TARGET_DIR/release/nuclear"
  local output_root="$CARGO_TARGET_DIR/runtime-cert-smoke"
  local python_cmd
  local latest_run

  if [ ! -x "$binary_path" ]; then
    printf 'runtime smoke requires a built CLI at %s\n' "$binary_path" >&2
    return 1
  fi

  rm -rf "$output_root"
  local harness_exit=0
  bash "$repo_root/scripts/run-harness.sh" \
    --lane runtime-cert \
    --binary-path "$binary_path" \
    --output-root "$output_root" \
    --task-filter "install-smoke,support-bundle-smoke" || harness_exit=$?

  latest_run="$(find "$output_root" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort | tail -n 1)"
  if [ "$harness_exit" -ne 0 ]; then
    if [ -n "$latest_run" ] && [ -f "$latest_run/runtime-cert/summary.md" ]; then
      printf 'runtime smoke summary:\n' >&2
      cat "$latest_run/runtime-cert/summary.md" >&2
    fi
    return "$harness_exit"
  fi

  if [ -z "$latest_run" ]; then
    printf 'runtime smoke did not produce an output directory\n' >&2
    return 1
  fi

  local summary_path="$latest_run/runtime-cert/summary.json"
  local summary_markdown_path="$latest_run/runtime-cert/summary.md"
  if [ ! -f "$summary_path" ] || [ ! -f "$summary_markdown_path" ]; then
    printf 'runtime smoke did not produce summary artifacts\n' >&2
    return 1
  fi

  if command -v python3 >/dev/null 2>&1; then
    python_cmd=python3
  elif command -v python >/dev/null 2>&1; then
    python_cmd=python
  else
    printf 'Python is required to validate runtime smoke artifacts\n' >&2
    return 1
  fi

  "$python_cmd" - <<'PY' "$summary_path"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    summary = json.load(handle)

if summary.get("failed") != 0 or summary.get("passed", 0) < 1:
    raise SystemExit("runtime smoke summary indicates failure")
PY
}

step "source LOC guard" bash "$repo_root/scripts/check-max-loc.sh"
step "dashboard checks" run_dashboard_checks
step "cargo fmt --all --check" cargo fmt --all --check
step "cargo check --workspace" cargo check --workspace
step "cargo test --workspace" cargo test --workspace
step "cargo build --release --bin nuclear" cargo build --release --bin nuclear
step "runtime smoke validation" run_runtime_smoke
step "cargo tree --workspace --duplicates" cargo tree --workspace --duplicates
step "workspace dependency drift" run_workspace_dependency_drift_check
step "release gate script tests" run_release_gate_script_tests
step "cargo audit" optional_cargo_tool audit
step "cargo deny check advisories licenses bans" optional_cargo_tool deny check advisories licenses bans
step "cargo outdated -R" optional_cargo_tool outdated -R
