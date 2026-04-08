#!/usr/bin/env bash
set -euo pipefail

install_dir="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
state_path="$install_dir/install-state.json"
binary_path="$install_dir/nuclear"

if [ ! -f "$state_path" ]; then
  printf 'rollback state was not found at %s\n' "$state_path" >&2
  exit 1
fi

extract_field() {
  field_name="$1"
  sed -n "s/.*\"$field_name\": \"\\([^\"]*\\)\".*/\\1/p" "$state_path" | head -n 1
}

rollback_binary="$(extract_field rollback_binary)"
if [ -z "$rollback_binary" ] || [ ! -f "$rollback_binary" ]; then
  printf 'rollback binary was not found at %s\n' "$rollback_binary" >&2
  exit 1
fi

if [ -x "$binary_path" ]; then
  "$binary_path" daemon stop >/dev/null 2>&1 || true
fi

tmp_path="$binary_path.rollback"
cp "$rollback_binary" "$tmp_path"
chmod 0755 "$tmp_path"
mv -f "$tmp_path" "$binary_path"

version="$("$binary_path" --version)"
printf 'Restored %s\n' "$version"
