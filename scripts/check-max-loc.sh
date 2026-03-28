#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
baseline_file="${1:-$repo_root/scripts/max-loc-baseline.txt}"
max_lines="${MAX_LOC_LINES:-2000}"

declare -A limits=()
if [[ -f "$baseline_file" ]]; then
  while IFS=$'\t' read -r path limit; do
    [[ -z "${path// }" ]] && continue
    [[ "${path:0:1}" == "#" ]] && continue
    path="${path%$'\r'}"
    limit="${limit%$'\r'}"
    limits["$path"]="$limit"
  done <"$baseline_file"
fi

offenders=0
while IFS= read -r file; do
  relative_path="${file#"$repo_root"/}"
  line_count="$(wc -l <"$file")"
  limit="${limits[$relative_path]:-$max_lines}"
  if (( line_count > limit )); then
    printf '%s\t%s\t%s\n' "$line_count" "$limit" "$relative_path"
    offenders=1
  fi
done < <(
  find "$repo_root/crates" -type f \( \
    -name '*.rs' -o \
    -name '*.js' -o \
    -name '*.cjs' -o \
    -name '*.mjs' \
  \) | sort
)

exit "$offenders"
