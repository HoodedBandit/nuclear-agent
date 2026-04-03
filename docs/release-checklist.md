# Release Checklist

Use this checklist before publishing a public release. Windows and Linux are both supported release platforms.

## 1. Repo and Product Readiness

- worktree reviewed and intentionally staged
- canonical product name is `Nuclear Agent`
- canonical command is `nuclear`
- no public release artifact depends on legacy names except migration paths and upgrade notes
- public repo contains only intentional product, build, test, and release assets

## 2. Core Verification

Workspace verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-workspace.ps1
```

GA verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-ga.ps1
```

Linux equivalents:

```bash
./scripts/verify-workspace.sh
./scripts/verify-ga.sh
```

`verify-workspace` covers:

- source LOC guard
- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo test --workspace`
- release build for `nuclear`
- runtime smoke
- dependency checks

`verify-ga` adds:

- strict clippy
- dashboard Playwright E2E
- full `runtime-cert`
- blocking `coding-deterministic`

## 3. Packaging and Trust

Final packaging:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```

Linux:

```bash
./scripts/finalize-release.sh --workspace .
```

Release outputs must include:

- packaged archive
- manifest
- checksum sidecars
- SPDX SBOM
- provenance statement
- release record
- signatures when signing is required

## 4. Operator Surface Signoff

The following surfaces remain release-relevant:

- onboarding and daemon lifecycle
- providers, aliases, and model selection
- dashboard chat and session flows
- plugins and plugin doctor
- connectors and approvals
- memory, missions, autonomy, autopilot, evolve, delegation, logs, and support-bundle export
- install, upgrade, rollback, reset, and recovery

For each blocking surface, confirm:

- a normal happy path
- a clear failure path
- a clear recovery path
- restart or persistence behavior

## 5. Manual Signoff

These remain manual signoff items and are not ordinary CI blockers:

- live hosted-provider login and model access
- live connector authentication and send or poll flows
- `coding-reference` against a real configured provider
- soak review with a live daemon token and workspace
- real signing with release keys

## 6. Publish Check

Before publishing:

- release notes describe the actual shipped behavior
- any remaining operational caveats are explicit
- packaged installs and source snapshot match the repo state being published
