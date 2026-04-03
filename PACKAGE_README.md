# Nuclear Agent Package

This bundle is a managed release package for Nuclear Agent.

## Included Files

- `bin/`: packaged binaries for the target platform
- `source/`: source snapshot for local rebuild fallback
- `install.ps1`, `install.cmd`, `install`: platform installers
- release metadata generated during packaging

## Install

Windows:

- `install.cmd`
- `install.ps1`

Linux:

- `install`

The installer places `nuclear` on the user PATH and writes managed install state for rollback and recovery.

## Upgrade Behavior

- fresh installs use the canonical Nuclear paths
- legacy managed installs are migrated one-way into canonical Nuclear paths
- new state is written only to the canonical `nuclear` layout

If the packaged binary is blocked by local application control, the installer can fall back to building from the packaged source tree.

## Verification

Installer smoke:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-smoke.ps1
```

Full release verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```
