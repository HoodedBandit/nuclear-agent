# GA Release Notes

## Nuclear Agent 0.8.0

This release publishes Nuclear Agent as a local Rust agent runtime for Windows and Linux.

## Highlights

- canonical `nuclear` command and package layout
- one-way migration from legacy managed installs into canonical Nuclear paths
- local daemon, CLI, TUI, and dashboard surfaces
- structured tool calling with trust and permission gates
- plugin, connector, memory, mission, delegation, and autonomy support
- rollback companions for managed installs
- redacted local support-bundle export
- deterministic release verification and packaging metadata

## Verification

Core GA verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-ga.ps1
```

Final release packaging:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```

Linux equivalents:

```bash
./scripts/verify-ga.sh
./scripts/finalize-release.sh --workspace .
```

## Manual Signoff Still Required

- live hosted-provider certification
- live external-connector certification
- `coding-reference` against a real configured provider
- soak review with a real daemon token and workspace
- final signing with release keys
