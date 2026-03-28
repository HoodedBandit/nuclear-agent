#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temp_root="$repo_root/target/install-smoke/linux"

step() {
  printf '\n==> %s\n' "$1"
}

assert_exists() {
  local path="$1"
  local label="$2"
  if [ ! -e "$path" ]; then
    printf '%s was not found at %s\n' "$label" "$path" >&2
    exit 1
  fi
}

cargo_target_root() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    case "$CARGO_TARGET_DIR" in
      /*) printf '%s\n' "$CARGO_TARGET_DIR" ;;
      *) printf '%s\n' "$repo_root/$CARGO_TARGET_DIR" ;;
    esac
    return
  fi

  printf '%s\n' "$repo_root/target"
}

run_installer() {
  local scenario_root="$1"
  local home_root="$scenario_root/home"

  mkdir -p "$home_root"
  HOME="$home_root" PATH="/usr/bin:/bin" NUCLEAR_SKIP_PATH_PERSIST=1 bash "$repo_root/install"
}

step "prepare Linux installer smoke workspace"
rm -rf "$temp_root"
mkdir -p "$temp_root"

cargo_target_root_value="$(cargo_target_root)"
step "build release compatibility binaries"
cargo build --release -p nuclear --bin nuclear --bin autism

fresh_root="$temp_root/fresh-default"
step "fresh install uses the default nuclear command path"
run_installer "$fresh_root"
fresh_install_dir="$fresh_root/home/.local/bin"
assert_exists "$fresh_install_dir/nuclear" "canonical binary"
assert_exists "$fresh_install_dir/autism" "legacy compatibility binary"
"$fresh_install_dir/nuclear" --version >/dev/null
"$fresh_install_dir/autism" --version >/dev/null

release_legacy_binary="$cargo_target_root_value/release/autism"
assert_exists "$release_legacy_binary" "legacy release compatibility binary"

upgrade_root="$temp_root/upgrade-legacy-default"
upgrade_install_dir="$upgrade_root/home/.local/bin"
mkdir -p "$upgrade_install_dir"
cp "$release_legacy_binary" "$upgrade_install_dir/autism"
chmod 0755 "$upgrade_install_dir/autism"

step "legacy default install upgrades in place"
run_installer "$upgrade_root"
assert_exists "$upgrade_install_dir/nuclear" "upgraded canonical binary"
assert_exists "$upgrade_install_dir/autism" "upgraded legacy compatibility binary"
"$upgrade_install_dir/nuclear" --version >/dev/null
"$upgrade_install_dir/autism" --version >/dev/null
