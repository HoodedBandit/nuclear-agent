#!/usr/bin/env bash
set -euo pipefail

if [ -d "${HOME:-}/.cargo/bin" ] && ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temp_root="${TMPDIR:-/tmp}/nuclear-install-smoke-linux-$$"

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

assert_not_exists() {
  local path="$1"
  local label="$2"
  if [ -e "$path" ]; then
    printf '%s should not exist at %s\n' "$label" "$path" >&2
    exit 1
  fi
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
    return
  fi
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
    return
  fi
  printf 'sha256sum or shasum is required\n' >&2
  exit 1
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
  local installer_root="$2"
  local home_root="$scenario_root/home"

  mkdir -p "$home_root"
  HOME="$home_root" PATH="/usr/bin:/bin" NUCLEAR_SKIP_PATH_PERSIST=1 bash "$installer_root/install" >/dev/null
}

step "prepare Linux installer smoke workspace"
rm -rf "$temp_root"
mkdir -p "$temp_root"

cargo_target_root_value="$(cargo_target_root)"
step "build release binaries"
cargo build --release -p nuclear --bin nuclear

fresh_root="$temp_root/fresh-default"
step "fresh install uses the default nuclear command path"
run_installer "$fresh_root" "$repo_root"
fresh_install_dir="$fresh_root/home/.local/bin"
assert_exists "$fresh_install_dir/nuclear" "canonical binary"
"$fresh_install_dir/nuclear" --version >/dev/null
if [ -e "$fresh_install_dir/autism" ]; then
  printf 'fresh installs must not leave a legacy launcher under %s\n' "$fresh_install_dir" >&2
  exit 1
fi

upgrade_root="$temp_root/upgrade-legacy-default"
upgrade_install_dir="$upgrade_root/home/.local/bin"
mkdir -p "$upgrade_install_dir"
printf 'legacy\n' >"$upgrade_install_dir/autism"
chmod 0755 "$upgrade_install_dir/autism"

step "legacy default install removes the legacy launcher in place"
run_installer "$upgrade_root" "$repo_root"
assert_exists "$upgrade_install_dir/nuclear" "upgraded canonical binary"
"$upgrade_install_dir/nuclear" --version >/dev/null
if [ -e "$upgrade_install_dir/autism" ]; then
  printf 'legacy upgrades must remove the old launcher from %s\n' "$upgrade_install_dir" >&2
  exit 1
fi

package_output_root="$temp_root/package-output"
step "package canonical Linux release bundle"
bash "$repo_root/scripts/package-release.sh" "$package_output_root" --clean

package_dir="$(find "$package_output_root" -mindepth 1 -maxdepth 1 -type d -name 'nuclear-*-linux-*-full' | sort | tail -n 1)"
if [ -z "$package_dir" ]; then
  printf 'could not locate the packaged Linux bundle under %s\n' "$package_output_root" >&2
  exit 1
fi

packaged_fresh_root="$temp_root/packaged-fresh-default"
step "packaged bundle fresh install uses the bundled nuclear binary"
run_installer "$packaged_fresh_root" "$package_dir"
packaged_fresh_install_dir="$packaged_fresh_root/home/.local/bin"
assert_exists "$packaged_fresh_install_dir/nuclear" "packaged canonical binary"
"$packaged_fresh_install_dir/nuclear" --version >/dev/null
if [ -e "$packaged_fresh_install_dir/autism" ]; then
  printf 'packaged fresh installs must not leave a legacy launcher under %s\n' "$packaged_fresh_install_dir" >&2
  exit 1
fi

packaged_upgrade_root="$temp_root/packaged-upgrade-legacy-default"
packaged_upgrade_install_dir="$packaged_upgrade_root/home/.local/bin"
mkdir -p "$packaged_upgrade_install_dir"
printf 'legacy\n' >"$packaged_upgrade_install_dir/autism"
chmod 0755 "$packaged_upgrade_install_dir/autism"

step "packaged bundle legacy install removes the legacy launcher in place"
run_installer "$packaged_upgrade_root" "$package_dir"
assert_exists "$packaged_upgrade_install_dir/nuclear" "packaged upgraded canonical binary"
"$packaged_upgrade_install_dir/nuclear" --version >/dev/null
if [ -e "$packaged_upgrade_install_dir/autism" ]; then
  printf 'packaged legacy upgrades must remove the old launcher from %s\n' "$packaged_upgrade_install_dir" >&2
  exit 1
fi

rollback_root="$temp_root/packaged-rollback"
rollback_install_dir="$rollback_root/home/.local/bin"

step "packaged install writes rollback state and restores the previous managed binary"
run_installer "$rollback_root" "$package_dir"
assert_exists "$rollback_install_dir/nuclear" "rollback scenario canonical binary"
assert_exists "$rollback_install_dir/install-state.json" "install state"
assert_exists "$rollback_install_dir/nuclear-rollback" "rollback script"
assert_not_exists "$rollback_install_dir/autism" "legacy launcher before rollback"
baseline_hash="$(sha256_file "$rollback_install_dir/nuclear")"

run_installer "$rollback_root" "$package_dir"
rollback_binary="$(sed -n 's/.*"rollback_binary": "\([^"]*\)".*/\1/p' "$rollback_install_dir/install-state.json" | head -n 1)"
if [ -z "$rollback_binary" ] || [ ! -f "$rollback_binary" ]; then
  printf 'rollback backup binary was not found at %s\n' "$rollback_binary" >&2
  exit 1
fi

printf 'broken\n' >"$rollback_install_dir/nuclear"
chmod 0755 "$rollback_install_dir/nuclear"
bash "$rollback_install_dir/nuclear-rollback" "$rollback_install_dir" >/dev/null
"$rollback_install_dir/nuclear" --version >/dev/null
restored_hash="$(sha256_file "$rollback_install_dir/nuclear")"
if [ "$restored_hash" != "$baseline_hash" ]; then
  printf 'rollback did not restore the previously installed binary\n' >&2
  exit 1
fi
assert_not_exists "$rollback_install_dir/autism" "legacy launcher after rollback"
