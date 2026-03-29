#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
task_file="$repo_root/benchmarks/release-eval/tasks.jsonl"
output_root="$repo_root/target/verify-beta/release-eval"
skip_e2e=0
skip_release_eval=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-e2e)
      skip_e2e=1
      ;;
    --skip-release-eval)
      skip_release_eval=1
      ;;
    --task-file)
      shift
      task_file="$1"
      ;;
    --output-root)
      shift
      output_root="$1"
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
step "phase 1 isolated runtime smoke" bash "$repo_root/scripts/verify-phase1.sh" "$repo_root/target/verify-workspace/release/nuclear"
step "phase 2 operator surface smoke" bash "$repo_root/scripts/verify-phase2.sh" "$repo_root/target/verify-workspace/release/nuclear"

if [ "$skip_e2e" -eq 0 ]; then
  step "dashboard Playwright E2E" npm run test:e2e
fi

if [ "$skip_release_eval" -eq 0 ]; then
  binary_path="$repo_root/target/verify-workspace/release/nuclear"
  if [ ! -x "$binary_path" ]; then
    printf 'release benchmark suite requires a built CLI at %s\n' "$binary_path" >&2
    exit 1
  fi
  step "release-eval benchmark suite" bash "$repo_root/scripts/run-bench.sh" "$task_file" "$binary_path" "$output_root"
fi
