#!/usr/bin/env bash
set -euo pipefail

if [ -d "${HOME:-}/.cargo/bin" ] && ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
remote="origin"
branch="main"
skip_local_verify=0
skip_push=0
skip_remote_verify=0
skip_finalize=0
timeout_seconds=3600
poll_interval_seconds=15

while [ "$#" -gt 0 ]; do
  case "$1" in
    --remote)
      shift
      remote="$1"
      ;;
    --branch)
      shift
      branch="$1"
      ;;
    --skip-local-verify)
      skip_local_verify=1
      ;;
    --skip-push)
      skip_push=1
      ;;
    --skip-remote-verify)
      skip_remote_verify=1
      ;;
    --skip-finalize)
      skip_finalize=1
      ;;
    --timeout-seconds)
      shift
      timeout_seconds="$1"
      ;;
    --poll-interval-seconds)
      shift
      poll_interval_seconds="$1"
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

assert_command() {
  local name="$1"
  local purpose="$2"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf '%s is required to %s\n' "$name" "$purpose" >&2
    exit 1
  fi
}

assert_clean_worktree() {
  local status
  status="$(git status --porcelain)"
  if [ -n "$status" ]; then
    printf 'release helper requires a clean worktree. Commit or stash changes first.\n' >&2
    exit 1
  fi
}

assert_branch() {
  local current_branch
  current_branch="$(git rev-parse --abbrev-ref HEAD)"
  if [ "$current_branch" != "$branch" ]; then
    printf "release helper must run from '%s'. Current branch is '%s'.\n" "$branch" "$current_branch" >&2
    exit 1
  fi
}

cd "$repo_root"

step "verify clean worktree" assert_clean_worktree
step "verify current branch" assert_branch

if [ "$skip_local_verify" -eq 0 ]; then
  step "local GA verification" bash "$repo_root/scripts/verify-ga.sh"
fi

if [ "$skip_push" -eq 0 ]; then
  step "push $branch to $remote" git push "$remote" "$branch"
fi

if [ "$skip_remote_verify" -eq 0 ] || [ "$skip_finalize" -eq 0 ]; then
  assert_command gh "query GitHub Actions and dispatch the release workflow"
  if command -v python3 >/dev/null 2>&1; then
    python_cmd="python3"
  elif command -v python >/dev/null 2>&1; then
    python_cmd="python"
  else
    printf 'Python is required to check GitHub verification state\n' >&2
    exit 1
  fi

  repo="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
  sha="$(git rev-parse HEAD)"

  if [ "$skip_remote_verify" -eq 0 ]; then
    step "wait for remote ga-verify success" env \
      GITHUB_TOKEN="$(gh auth token)" \
      "$python_cmd" "$repo_root/scripts/require_green_ga.py" \
      --repo "$repo" \
      --sha "$sha" \
      --branch "$branch" \
      --wait \
      --timeout-seconds "$timeout_seconds" \
      --poll-interval-seconds "$poll_interval_seconds"
  fi

  if [ "$skip_finalize" -eq 0 ]; then
    step "dispatch finalize-release" gh workflow run finalize-release.yml --ref "$branch"
  fi
fi
