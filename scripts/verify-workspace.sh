#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

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

step "cargo check --workspace" cargo check --workspace
step "cargo test --workspace" cargo test --workspace
step "cargo build --release --bin autism" cargo build --release --bin autism
step "cargo tree --workspace --duplicates" cargo tree --workspace --duplicates
step "cargo audit" optional_cargo_tool audit
step "cargo deny check advisories licenses bans" optional_cargo_tool deny check advisories licenses bans
step "cargo outdated -R" optional_cargo_tool outdated -R
