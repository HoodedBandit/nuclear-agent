#!/usr/bin/env bash
set -euo pipefail

if [ -d "${HOME:-}/.cargo/bin" ] && ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_root="${1:-$repo_root/dist}"
clean_flag="${2:-}"
require_signing=0

if [ "${3:-}" = "--require-signing" ]; then
  require_signing=1
fi

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

  if [ -x "$canonical" ]; then
    printf '%s\n' "$canonical"
    return
  fi

  step "Building release binary"
  cargo build --release -p nuclear --bin nuclear
  if [ ! -x "$canonical" ]; then
    printf 'release build completed but the expected binary was not found\n' >&2
    exit 1
  fi
  printf '%s\n' "$canonical"
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

git_tree_state_json() {
  if ! command -v git >/dev/null 2>&1; then
    printf '{"commit_dirty": false, "dirty_paths": []}\n'
    return
  fi

  status="$(git -C "$repo_root" status --short --untracked-files=all 2>/dev/null || true)"
  if [ -z "$status" ]; then
    printf '{"commit_dirty": false, "dirty_paths": []}\n'
    return
  fi

  if command -v python3 >/dev/null 2>&1; then
    python_emit=python3
  elif command -v python >/dev/null 2>&1; then
    python_emit=python
  else
    printf 'Python is required to encode git tree state\n' >&2
    exit 1
  fi

  "$python_emit" - <<'PY' "$status"
import json
import sys

paths = []
for line in sys.argv[1].splitlines():
    trimmed = line[3:].strip() if len(line) > 3 else line.strip()
    if trimmed:
        paths.append(trimmed)
print(json.dumps({"commit_dirty": True, "dirty_paths": paths}))
PY
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
sbom_path="$output_root/$bundle_name.sbom.spdx.json"
provenance_path="$output_root/$bundle_name.provenance.json"
signing_status_path="$output_root/$bundle_name.signing.json"
manifest_path="$output_root/$bundle_name.manifest.json"

mkdir -p "$output_root"
if [ "$clean_flag" = "--clean" ]; then
  rm -rf "$bundle_dir" "$archive_path" "$archive_hash_path" "$sbom_path" "$provenance_path" "$signing_status_path" "$manifest_path"
fi

mapfile -t binaries < <(ensure_release_binaries)
canonical_binary="${binaries[0]}"

mkdir -p "$bundle_dir/bin/$platform_tag" "$bundle_dir/source"

step "Copying packaged installer surface"
cp "$repo_root/install" "$bundle_dir/install"
cp "$repo_root/install.cmd" "$bundle_dir/install.cmd"
cp "$repo_root/install.ps1" "$bundle_dir/install.ps1"
cp "$repo_root/PACKAGE_README.md" "$bundle_dir/README.md"

step "Copying bundled release binaries"
cp "$canonical_binary" "$bundle_dir/bin/$platform_tag/nuclear"
chmod 0755 "$bundle_dir/bin/$platform_tag/nuclear"

step "Copying source snapshot"
for item in \
  .cargo \
  .github \
  benchmarks \
  crates \
  docs \
  harness \
  scripts \
  tests \
  .gitignore \
  Cargo.lock \
  Cargo.toml \
  LICENSE \
  deny.toml \
  install \
  install.cmd \
  install.ps1 \
  package-lock.json \
  package.json \
  playwright.config.cjs \
  README.md \
  PACKAGE_README.md \
  ui/dashboard/eslint.config.js \
  ui/dashboard/index.html \
  ui/dashboard/package-lock.json \
  ui/dashboard/package.json \
  ui/dashboard/src \
  ui/dashboard/tsconfig.json \
  ui/dashboard/vite.config.ts
do
  copy_snapshot_item "$item"
done

canonical_hash="$(sha256_file "$bundle_dir/bin/$platform_tag/nuclear")"
commit_sha="$(git_commit_sha)"
created_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
git_tree_state="$(git_tree_state_json)"
if command -v python3 >/dev/null 2>&1; then
  python_manifest=python3
else
  python_manifest=python
fi
commit_dirty="$(printf '%s' "$git_tree_state" | "$python_manifest" -c "import json,sys; print('true' if json.load(sys.stdin)['commit_dirty'] else 'false')")"
dirty_paths_json="$(printf '%s' "$git_tree_state" | "$python_manifest" -c "import json,sys; print(json.dumps(json.load(sys.stdin)['dirty_paths']))")"

cat >"$bundle_dir/release-manifest.json" <<EOF
{
  "name": "$bundle_name",
  "version": "$version",
  "platform": "$platform_tag",
  "created_at": "$created_at",
  "commit_sha": "$commit_sha",
  "commit_dirty": $commit_dirty,
  "dirty_paths": $dirty_paths_json,
  "binaries": {
    "canonical": {
      "name": "nuclear",
      "sha256": "$canonical_hash"
    }
  },
  "install": {
    "canonical_command": "nuclear",
    "fresh_root": "~/.local/bin"
  }
}
EOF

step "Compressing packaged bundle"
rm -f "$archive_path"
tar -czf "$archive_path" -C "$output_root" "$bundle_name"
archive_hash="$(sha256_file "$archive_path")"
printf '%s  %s\n' "$archive_hash" "$(basename "$archive_path")" >"$archive_hash_path"

if command -v python3 >/dev/null 2>&1; then
  python_cmd=python3
elif command -v python >/dev/null 2>&1; then
  python_cmd=python
else
  printf 'Python is required to generate release metadata\n' >&2
  exit 1
fi

step "Generating SBOM"
"$python_cmd" "$repo_root/scripts/generate_sbom.py" \
  --repo-root "$repo_root" \
  --bundle-name "$bundle_name" \
  --version "$version" \
  --platform "$platform_tag" \
  --output-path "$sbom_path"

cat >"$manifest_path" <<EOF
{
  "name": "$bundle_name",
  "version": "$version",
  "platform": "$platform_tag",
  "created_at": "$created_at",
  "commit_sha": "$commit_sha",
  "commit_dirty": $commit_dirty,
  "dirty_paths": $dirty_paths_json,
  "bundle_dir": "$bundle_dir",
  "archive_path": "$archive_path",
  "archive_sha256": "$archive_hash",
  "checksum_path": "$archive_hash_path",
  "package_readme": "$bundle_dir/README.md",
  "internal_manifest": "$bundle_dir/release-manifest.json",
  "sbom_path": "$sbom_path",
  "provenance_path": "$provenance_path",
  "signing_status": "$signing_status_path",
  "signing_required": $([ "$require_signing" -eq 1 ] && printf 'true' || printf 'false'),
  "signing_hook": "${NUCLEAR_SIGNING_HOOK:-}"
}
EOF

step "Generating provenance"
"$python_cmd" "$repo_root/scripts/generate_provenance.py" \
  --manifest-path "$manifest_path" \
  --archive-path "$archive_path" \
  --checksum-path "$archive_hash_path" \
  --sbom-path "$sbom_path" \
  --output-path "$provenance_path"

step "Collecting signatures"
"$python_cmd" "$repo_root/scripts/sign_artifacts.py" \
  --manifest-path "$manifest_path" \
  --artifacts "$archive_path" "$archive_hash_path" "$manifest_path" "$sbom_path" "$provenance_path" \
  --status-path "$signing_status_path"

if [ "$require_signing" -eq 1 ]; then
  if command -v python3 >/dev/null 2>&1; then
    python_check=python3
  else
    python_check=python
  fi
  "$python_check" - <<'PY' "$signing_status_path"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    status = json.load(handle)

if not status.get("enabled"):
    raise SystemExit("Signing is required but NUCLEAR_SIGNING_HOOK was not configured.")
if not status.get("signatures"):
    raise SystemExit("Signing is required but no artifact signatures were recorded.")
PY
fi

printf 'Package output written to %s\n' "$bundle_dir"
printf 'Archive written to %s\n' "$archive_path"
printf 'Manifest written to %s\n' "$manifest_path"
