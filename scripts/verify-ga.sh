#!/usr/bin/env bash
set -euo pipefail

if [ -d "${HOME:-}/.cargo/bin" ] && ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
task_file="$repo_root/harness/tasks/coding/tasks.json"
runtime_cert_output_root="$repo_root/target/verify-ga/runtime-cert"
coding_output_root="$repo_root/target/verify-ga/coding-deterministic"
skip_e2e=0
skip_deterministic_coding=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-e2e)
      skip_e2e=1
      ;;
    --skip-deterministic-coding)
      skip_deterministic_coding=1
      ;;
    --skip-release-eval)
      printf 'warning: --skip-release-eval is deprecated; use --skip-deterministic-coding\n' >&2
      skip_deterministic_coding=1
      ;;
    --task-file)
      shift
      task_file="$1"
      ;;
    --output-root)
      shift
      runtime_cert_output_root="$1/runtime-cert"
      coding_output_root="$1/coding-deterministic"
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      exit 1
      ;;
  esac
  shift
done

step() {
  printf '\n==> %s\n' "$1"
  shift
  "$@"
}

step "baseline workspace verification" bash "$repo_root/scripts/verify-workspace.sh"
step "runtime certification lane" bash "$repo_root/scripts/run-harness.sh" \
  --lane runtime-cert \
  --binary-path "$repo_root/target/verify-workspace/release/nuclear" \
  --output-root "$runtime_cert_output_root"
step "cargo clippy --workspace --all-targets --all-features -- -D warnings" cargo clippy --workspace --all-targets --all-features --target-dir "$repo_root/target/verify-workspace" -- -D warnings

if [ "$skip_e2e" -eq 0 ]; then
  step "dashboard Playwright E2E" npm run test:e2e
fi

if [ "$skip_deterministic_coding" -eq 0 ]; then
  binary_path="$repo_root/target/verify-workspace/release/nuclear"
  if [ ! -x "$binary_path" ]; then
    printf 'deterministic coding harness requires a built CLI at %s\n' "$binary_path" >&2
    exit 1
  fi
  step "deterministic coding harness" bash "$repo_root/scripts/run-harness.sh" \
    --lane coding-deterministic \
    --task-file "$task_file" \
    --binary-path "$binary_path" \
    --output-root "$coding_output_root"
fi
