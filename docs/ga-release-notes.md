# GA Release Notes

## Summary

Nuclear Agent `0.8.3` is a security and stability patch release for the local Rust agent runtime on Windows and Linux.

## Platform Support

- Windows and Linux are both blocking release platforms.
- Fresh installs ship only the canonical `nuclear` command.
- Legacy `autism` and `Agent Builder` installs are migrated into canonical Nuclear-managed roots during upgrade.

## Operational Highlights

- managed filesystem paths are now validated more aggressively across plugin install and removal flows, support-bundle export, and packaged updater staging so traversal-style inputs fail before any write occurs
- provider and OAuth error surfaces now redact bearer tokens, refresh tokens, API keys, and password-like fields before they reach operator-visible errors, persisted artifacts, or harness summaries
- OAuth provider configuration now rejects remote `http` authorization and token endpoints while still allowing explicit loopback development callbacks on `localhost`, `127.0.0.1`, and `::1`
- support-bundle and updater state artifacts are now written through explicit redaction helpers so exported diagnostics remain useful without leaking credentials
- Python harness summaries and console output now sanitize token-like values before writing JSON artifacts or printing status lines during verification
- new regression coverage locks these fixes in at the Rust unit and integration layers, the Python harness test suite, and the existing dashboard and release verification gates
- packaged installs continue to check GitHub Releases and apply updates through `nuclear update`, interactive `/update`, and the dashboard system workbench

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
