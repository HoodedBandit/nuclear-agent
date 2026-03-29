#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_root="${1:-$repo_root/dist}"
clean_flag="${2:-}"

step() {
  printf '\n==> %s\n' "$1"
}

workspace_version() {
  awk '
    /^\[workspace\.package\]$/ { in_section=1; next }
    /^\[/ { in_section=0 }
    in_section && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$repo_root/Cargo.toml"
}

arch_tag() {
  case "$(uname -m)" in
    aarch64|arm64) printf 'arm64\n' ;;
    *) printf 'x64\n' ;;
  esac
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

ensure_release_binaries() {
  local target_root
  target_root="$(cargo_target_root)"
  local canonical="$target_root/release/nuclear"
  local legacy="$target_root/release/autism"

  if [ -x "$canonical" ] && [ -x "$legacy" ]; then
    printf '%s\n%s\n' "$canonical" "$legacy"
    return
  fi

  step "Building release compatibility binaries"
  cargo build --release -p nuclear --bin nuclear --bin autism
  if [ ! -x "$canonical" ] || [ ! -x "$legacy" ]; then
    printf 'release build completed but expected binaries were not found\n' >&2
    exit 1
  fi
  printf '%s\n%s\n' "$canonical" "$legacy"
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

git_commit_sha() {
  git -C "$repo_root" rev-parse HEAD 2>/dev/null || true
}

copy_snapshot_item() {
  local relative_path="$1"
  local source_path="$repo_root/$relative_path"
  local destination_path="$bundle_dir/source/$relative_path"

  if [ ! -e "$source_path" ]; then
    return
  fi

  mkdir -p "$(dirname "$destination_path")"
  cp -R "$source_path" "$destination_path"
}

version="$(workspace_version)"
if [ -z "$version" ]; then
  printf 'could not determine workspace version from Cargo.toml\n' >&2
  exit 1
fi

platform_tag="linux-$(arch_tag)"
bundle_name="nuclear-$version-$platform_tag-full"
bundle_dir="$output_root/$bundle_name"
archive_path="$output_root/$bundle_name.tar.gz"
archive_hash_path="$output_root/$bundle_name.tar.gz.sha256.txt"
manifest_path="$output_root/$bundle_name.manifest.json"

mkdir -p "$output_root"
if [ "$clean_flag" = "--clean" ]; then
  rm -rf "$bundle_dir" "$archive_path" "$archive_hash_path" "$manifest_path"
fi

mapfile -t binaries < <(ensure_release_binaries)
canonical_binary="${binaries[0]}"
legacy_binary="${binaries[1]}"

mkdir -p "$bundle_dir/bin/$platform_tag" "$bundle_dir/source"

step "Copying packaged installer surface"
cp "$repo_root/install" "$bundle_dir/install"
cp "$repo_root/install.cmd" "$bundle_dir/install.cmd"
cp "$repo_root/install.ps1" "$bundle_dir/install.ps1"
cp "$repo_root/PACKAGE_README.md" "$bundle_dir/README.md"

step "Copying bundled release binaries"
cp "$canonical_binary" "$bundle_dir/bin/$platform_tag/nuclear"
cp "$legacy_binary" "$bundle_dir/bin/$platform_tag/autism"
chmod 0755 "$bundle_dir/bin/$platform_tag/nuclear" "$bundle_dir/bin/$platform_tag/autism"

step "Copying source snapshot"
for item in \
  .cargo \
  benchmarks \
  crates \
  docs \
  scripts \
  tests \
  Cargo.lock \
  Cargo.toml \
  deny.toml \
  package-lock.json \
  package.json \
  playwright.config.cjs \
  PROJECT_REVIEW.md \
  README.md \
  RECOVERY_REPORT.md \
  WORKTREE_LOG_2026-03-13.md \
  PACKAGE_README.md
do
  copy_snapshot_item "$item"
done

canonical_hash="$(sha256_file "$bundle_dir/bin/$platform_tag/nuclear")"
legacy_hash="$(sha256_file "$bundle_dir/bin/$platform_tag/autism")"
commit_sha="$(git_commit_sha)"
created_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

cat >"$bundle_dir/release-manifest.json" <<EOF
{
  "name": "$bundle_name",
  "version": "$version",
  "platform": "$platform_tag",
  "created_at": "$created_at",
  "commit_sha": "$commit_sha",
  "binaries": {
    "canonical": {
      "name": "nuclear",
      "sha256": "$canonical_hash"
    },
    "legacy": {
      "name": "autism",
      "sha256": "$legacy_hash"
    }
  },
  "install": {
    "canonical_command": "nuclear",
    "legacy_command": "autism",
    "fresh_root": "~/.local/bin",
    "legacy_root": "~/.local/bin"
  }
}
EOF

step "Compressing packaged bundle"
rm -f "$archive_path"
tar -czf "$archive_path" -C "$output_root" "$bundle_name"
archive_hash="$(sha256_file "$archive_path")"
printf '%s  %s\n' "$archive_hash" "$(basename "$archive_path")" >"$archive_hash_path"

cat >"$manifest_path" <<EOF
{
  "name": "$bundle_name",
  "version": "$version",
  "platform": "$platform_tag",
  "created_at": "$created_at",
  "commit_sha": "$commit_sha",
  "bundle_dir": "$bundle_dir",
  "archive_path": "$archive_path",
  "archive_sha256": "$archive_hash",
  "checksum_path": "$archive_hash_path",
  "package_readme": "$bundle_dir/README.md",
  "internal_manifest": "$bundle_dir/release-manifest.json"
}
EOF

printf 'Package output written to %s\n' "$bundle_dir"
printf 'Archive written to %s\n' "$archive_path"
printf 'Manifest written to %s\n' "$manifest_path"
