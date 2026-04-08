# Nuclear Agent Package

This package is the Windows release bundle for Nuclear Agent.

## Install

Run one of the packaged installers from this directory:

- `install.cmd`
- `install.ps1`

The installer places `nuclear` on the user PATH for normal use and migrates any
legacy managed install into the canonical Nuclear root.

## What Is Included

- `bin/windows-x64/`: bundled Windows release binaries
- `source/`: source snapshot used for fallback local builds
- `install.ps1` and `install.cmd`: Windows installers
- `install`: Linux installer copied for source-tree parity

## Notes

- Fresh installs default to the canonical `nuclear` install root.
- Existing legacy `autism` installs are migrated into the canonical Nuclear root
  during upgrade.
- If Windows application control blocks the bundled binary, `install.ps1` falls
  back to building from `source/` and installs `rustup` automatically when
  needed.
- Packaged installs write rollback state and install rollback companions.

## Verify

From the repo root, the packaged installer path is covered by:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-smoke.ps1
```

The full GA verification stack is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```
