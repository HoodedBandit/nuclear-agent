# Release Checklist

Use this checklist before cutting a public GA release or shipping a packaged build. Windows and Linux are both blocking release platforms.

## Phase 1. Rename Cutover and Upgrade Safety

- Ensure the worktree is clean: `git status --short`
- Confirm the canonical CLI and package name is `nuclear`
- Confirm remaining `autism` or `Agent Builder` references exist only in migration code paths, upgrade notes, or compatibility tests
- Validate fresh installs land in canonical Nuclear roots on Windows and Linux
- Validate managed upgrades migrate legacy install roots, state roots, and saved credentials into canonical Nuclear roots without data loss
- Validate managed upgrades remove legacy launchers from the canonical install directory after migration
- Confirm docs, packages, manifests, scripts, and examples use `Nuclear Agent` and `nuclear`

## Phase 2. Runtime, Surface, and Local Ops Hardening

Every operator-facing surface remains in scope:

- dashboard and session workflows
- providers and aliases
- plugins and plugin doctor
- connectors, approvals, and polling or sending flows
- memory, missions, autonomy, autopilot, evolve, delegation, MCP/apps, logs, and doctor
- install, rollback, reset, recovery, and support-bundle export

For each blocking surface, require:

- one documented happy path
- one clear auth or failure path
- one clear recovery path
- one restart or persistence validation

Local operations signoff:

- packaged installers write `install-state.json`
- packaged installers install rollback companions
- rollback restores the previous managed binary
- `nuclear support-bundle` exports redacted local diagnostics with logs, sessions, config summary, doctor output, and daemon status when available

## Phase 3. GA Verification and Signoff

### Core automated verification

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-ga.ps1
```

Linux:

```bash
./scripts/verify-ga.sh
```

`verify-ga` covers:

- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo test --workspace`
- release build for the shipping binary
- fast runtime smoke in `verify-workspace`
- full `runtime-cert` lane
- strict clippy
- dashboard Playwright E2E
- blocking `coding-deterministic`

### Final release packaging

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```

Linux:

```bash
./scripts/finalize-release.sh --workspace .
```

`finalize-release` runs `verify-ga`, packages the release bundle, emits the SBOM and provenance, requires signatures unless explicitly skipped, optionally runs `coding-reference`, optionally runs the soak lane, and writes a timestamped production release record under `target/release-records/`.

### Supply-chain requirements

- release archive
- checksum sidecar
- bundle manifest
- SPDX SBOM
- provenance statement
- detached signatures for release artifacts
- production release record

Signing is supplied through `NUCLEAR_SIGNING_HOOK`. The final release step must fail if signing is required and the hook is not configured.

### Manual certification

- complete live-account provider signoff for every shipped hosted provider
- complete live connector signoff for every shipped external connector family
- review soak output plus the `runtime-cert`, `coding-deterministic`, and `coding-reference` summaries for latency drift, tool-use drift, and session regressions
- confirm release notes match the shipped behavior exactly
- record any explicit deferred operational caveats in the release notes before publishing
