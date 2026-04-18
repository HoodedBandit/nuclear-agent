# GA Release Notes

## Summary

Nuclear Agent `0.8.1` is a stability and operator-control patch release for the local Rust agent runtime on Windows and Linux.

## Platform Support

- Windows and Linux are both blocking release platforms.
- Fresh installs ship only the canonical `nuclear` command.
- Legacy `autism` and `Agent Builder` installs are migrated into canonical Nuclear-managed roots during upgrade.

## Operational Highlights

- packaged installs can now check GitHub Releases and apply updates through `nuclear update`, interactive `/update`, and the dashboard system workbench
- the updater stages release bundles, verifies checksums, hands off to a dedicated update helper, and restarts the daemon after replacement
- Windows PowerShell verification now resolves `npm.cmd` directly so release checks still work on machines with restrictive script execution policy
- managed installs now write rollback state and install rollback companions
- `nuclear support-bundle` exports redacted local diagnostics for offline incident triage
- release verification now includes deterministic `release-eval`, packaged rollback smoke, support-bundle smoke, dashboard E2E, SBOM generation, provenance generation, and production release records
- packaged release bundles now emit checksum, SBOM, provenance, and detached signature sidecars

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
