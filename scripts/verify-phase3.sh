#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_output_root="$repo_root/target/phase3/package"
release_record_output_root="$repo_root/target/release-records"
soak_output_root="$repo_root/target/soak"
skip_e2e=0
skip_release_eval=0
skip_soak=0
token="${AGENT_TOKEN:-}"
base_url="${AGENT_BASE_URL:-http://127.0.0.1:42690}"
workspace="${AGENT_WORKSPACE_PATH:-}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-e2e)
      skip_e2e=1
      ;;
    --skip-release-eval)
      skip_release_eval=1
      ;;
    --skip-soak)
      skip_soak=1
      ;;
    --token)
      shift
      token="$1"
      ;;
    --base-url)
      shift
      base_url="$1"
      ;;
    --workspace)
      shift
      workspace="$1"
      ;;
    --package-output-root)
      shift
      package_output_root="$1"
      ;;
    --release-record-output-root)
      shift
      release_record_output_root="$1"
      ;;
    --soak-output-root)
      shift
      soak_output_root="$1"
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

verify_beta_args=()
if [ "$skip_e2e" -eq 1 ]; then
  verify_beta_args+=(--skip-e2e)
fi
if [ "$skip_release_eval" -eq 1 ]; then
  verify_beta_args+=(--skip-release-eval)
fi

step "beta verification" bash "$repo_root/scripts/verify-beta.sh" "${verify_beta_args[@]}"
step "package canonical release bundle" bash "$repo_root/scripts/package-release.sh" "$package_output_root" --clean

soak_root_arg="$soak_output_root"
if [ "$skip_soak" -eq 0 ]; then
  if [ -z "$token" ]; then
    printf 'Phase 3 soak requires --token or AGENT_TOKEN unless --skip-soak is passed.\n' >&2
    exit 1
  fi
  step "daemon soak harness" bash "$repo_root/scripts/run-soak.sh" "$token" "$base_url" 30 1000 "$workspace" "$soak_output_root"
fi

step "write release record" bash "$repo_root/scripts/write-release-record.sh" \
  "$package_output_root" \
  "$repo_root/target/verify-workspace/benchmarks-smoke" \
  "$repo_root/target/verify-beta/release-eval" \
  "$soak_root_arg" \
  "$release_record_output_root"
