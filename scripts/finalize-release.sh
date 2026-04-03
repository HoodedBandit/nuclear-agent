#!/usr/bin/env bash
set -euo pipefail

if [ -d "${HOME:-}/.cargo/bin" ] && ! command -v cargo >/dev/null 2>&1; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
package_output_root="$repo_root/target/release/package"
release_record_output_root="$repo_root/target/release-records"
soak_output_root="$repo_root/target/soak"
skip_e2e=0
skip_deterministic_coding=0
skip_coding_reference=0
skip_release_eval=0
skip_soak=0
skip_signing=0
task_file="$repo_root/harness/tasks/coding/tasks.json"
reference_profile=""
alias=""
provider_id=""
model=""
provider_kind=""
reference_base_url=""
api_key_env=""
token="${AGENT_TOKEN:-}"
base_url="${AGENT_BASE_URL:-http://127.0.0.1:42690}"
workspace="${AGENT_WORKSPACE_PATH:-}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --skip-e2e)
      skip_e2e=1
      ;;
    --skip-deterministic-coding)
      skip_deterministic_coding=1
      ;;
    --skip-coding-reference)
      skip_coding_reference=1
      ;;
    --skip-release-eval)
      printf 'warning: --skip-release-eval is deprecated; use --skip-deterministic-coding and/or --skip-coding-reference\n' >&2
      skip_deterministic_coding=1
      skip_coding_reference=1
      skip_release_eval=1
      ;;
    --task-file)
      shift
      task_file="$1"
      ;;
    --reference-profile)
      shift
      reference_profile="$1"
      ;;
    --alias)
      shift
      alias="$1"
      ;;
    --provider-id)
      shift
      provider_id="$1"
      ;;
    --model)
      shift
      model="$1"
      ;;
    --provider-kind)
      shift
      provider_kind="$1"
      ;;
    --reference-base-url)
      shift
      reference_base_url="$1"
      ;;
    --api-key-env)
      shift
      api_key_env="$1"
      ;;
    --skip-soak)
      skip_soak=1
      ;;
    --skip-signing)
      skip_signing=1
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

verify_args=()
if [ "$skip_e2e" -eq 1 ]; then
  verify_args+=(--skip-e2e)
fi
if [ "$skip_deterministic_coding" -eq 1 ]; then
  verify_args+=(--skip-deterministic-coding)
fi
verify_args+=(--task-file "$task_file")

package_args=("$package_output_root" --clean)
if [ "$skip_signing" -eq 0 ]; then
  package_args+=(--require-signing)
fi

step "GA verification" bash "$repo_root/scripts/verify-ga.sh" "${verify_args[@]}"
step "package canonical release bundle" bash "$repo_root/scripts/package-release.sh" "${package_args[@]}"

if [ "$skip_coding_reference" -eq 0 ]; then
  reference_args=(
    --lane coding-reference
    --binary-path "$repo_root/target/verify-workspace/release/nuclear"
    --output-root "$repo_root/target/finalize-release/coding-reference"
    --task-file "$task_file"
  )
  if [ -n "$reference_profile" ]; then
    reference_args+=(--profile "$reference_profile")
  fi
  if [ -n "$alias" ]; then
    reference_args+=(--alias "$alias")
  fi
  if [ -n "$provider_id" ]; then
    reference_args+=(--provider-id "$provider_id")
  fi
  if [ -n "$model" ]; then
    reference_args+=(--model "$model")
  fi
  if [ -n "$provider_kind" ]; then
    reference_args+=(--provider-kind "$provider_kind")
  fi
  if [ -n "$reference_base_url" ]; then
    reference_args+=(--base-url "$reference_base_url")
  fi
  if [ -n "$api_key_env" ]; then
    reference_args+=(--api-key-env "$api_key_env")
  fi
  step "reference coding harness" bash "$repo_root/scripts/run-harness.sh" "${reference_args[@]}"
fi

if [ "$skip_soak" -eq 0 ]; then
  if [ -z "$token" ]; then
    printf 'Finalize release requires --token or AGENT_TOKEN unless --skip-soak is passed.\n' >&2
    exit 1
  fi
  step "daemon soak harness" bash "$repo_root/scripts/run-harness.sh" \
    --lane soak \
    --token "$token" \
    --soak-base-url "$base_url" \
    --workspace "$workspace" \
    --output-root "$soak_output_root"
fi

step "write production release record" bash "$repo_root/scripts/write-release-record.sh" \
  "$package_output_root" \
  "$repo_root/target/verify-ga/runtime-cert" \
  "$repo_root/target/verify-ga/coding-deterministic" \
  "$repo_root/target/finalize-release/coding-reference" \
  "$soak_output_root" \
  "$release_record_output_root" \
  "$repo_root/docs/ga-release-notes.md" \
  "$repo_root/docs/release-checklist.md" \
  "$([ "$skip_coding_reference" -eq 0 ] && printf '%s' '--require-coding-reference')"
