# Nuclear Agent Package

This bundle is the managed install surface for Nuclear Agent. It is meant for people who want the packaged runtime, rollback support, and a source snapshot for rebuild fallback without cloning the full repo first.

## What's In The Bundle

- `bin/`: packaged binaries for the target platform
- `source/`: source snapshot used for rebuild fallback and provenance
- `install.ps1`, `install.cmd`, `install`: platform installers
- release metadata, SBOM, provenance, and signing status outputs generated during packaging

## Install

Windows:

```powershell
.\install.cmd
```

or

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

Linux:

```bash
./install
```

The installer places `nuclear` on the user PATH, records managed install metadata, and enables rollback/recovery flows for future updates.

## Upgrade Behavior

- fresh installs use the canonical Nuclear paths
- legacy managed installs are migrated one-way into canonical Nuclear paths
- new state is written only to the canonical `nuclear` layout

If local application control blocks the packaged binary, the installer can fall back to rebuilding from the packaged source snapshot.

## Verification

Installer smoke:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-smoke.ps1
```

For the full release flow, use the repo-level release scripts described in the main [README](README.md) and [docs/release-checklist.md](docs/release-checklist.md).
