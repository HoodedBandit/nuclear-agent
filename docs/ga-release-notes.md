# GA Release Notes

## Summary

Nuclear Agent `0.8.2` is a dashboard stabilization and operator-quality patch release for the local Rust agent runtime on Windows and Linux.

## Platform Support

- Windows and Linux are both blocking release platforms.
- Fresh installs ship only the canonical `nuclear` command.
- Legacy `autism` and `Agent Builder` installs are migrated into canonical Nuclear-managed roots during upgrade.

## Operational Highlights

- the React dashboard shell now follows the OpenClaw-style navigation model more closely with grouped menus, a cleaner top bar, tighter route chrome, and reduced filler copy
- dashboard wiring is regression-covered end to end for auth, chat, attachments, providers, aliases, connectors, plugins, support-bundle, and update-check flows
- dashboard visual verification now includes bounded desktop, tablet, and mobile shell checks so overflow and clipping regressions fail in CI instead of shipping
- packaged installs continue to check GitHub Releases and apply updates through `nuclear update`, interactive `/update`, and the dashboard system workbench
- the packaged updater path and release mocks were realigned so post-release update checks continue to validate against the next available build instead of the just-shipped version
- Windows PowerShell verification now retries locked dashboard E2E workspaces more defensively, reducing flaky cleanup failures during repeated UI runs
- packaged release bundles continue to emit checksum, SBOM, provenance, and detached signature sidecars
- release operations now have a dedicated `release-main` helper plus a GitHub-side gate that blocks `finalize-release` until the exact commit has already passed `ga-verify`

## Release Commands

Windows core GA verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-ga.ps1
```

Windows final packaging and release-record generation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```

Linux core GA verification:

```bash
./scripts/verify-ga.sh
```

Linux final packaging and release-record generation:

```bash
./scripts/finalize-release.sh --workspace .
```

## Remaining Manual Signoff

- live hosted-provider certification
- live external-connector certification
- soak review with a real daemon token and configured workspace
