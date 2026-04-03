# Operations

## Core Health Checks

Run these before changing runtime state:

```powershell
nuclear doctor
nuclear daemon status
nuclear plugin doctor
```

If the daemon is running, the dashboard also exposes live status, logs, plugins, providers, and control state.

## Routine Update Flow

1. Pause autonomy, autopilot, evolve, or mission work if active.
2. Run `nuclear doctor`.
3. Update the binary or reinstall the package.
4. Run `nuclear plugin doctor`.
5. Update any plugin package whose source or review state changed.
6. Restart the daemon and re-run `nuclear doctor`.

## Rollback

Managed packaged installs write rollback metadata and install rollback companions.

Windows:

```powershell
& "$env:LOCALAPPDATA\Programs\NuclearAI\Nuclear\bin\nuclear-rollback.ps1"
```

Linux:

```bash
~/.local/bin/nuclear-rollback
```

Rollback restores the previous managed binary, validates it with `--version`, and preserves the canonical managed install layout.

After rollback:

1. Run `nuclear doctor`.
2. Run `nuclear plugin doctor`.
3. Reinstall or update any plugin package that no longer matches the restored host version.

## Support Bundle

Export a redacted local diagnostics bundle before escalation or manual triage:

```powershell
nuclear support-bundle
nuclear support-bundle --output-dir .\tmp\support-bundle --log-limit 200 --session-limit 25
```

The support bundle includes:

- doctor output
- daemon status when available
- config summary with secrets removed
- recent session metadata
- recent logs
- install-state metadata
- migration metadata when present

## Recovery

If the daemon becomes unhealthy:

1. Run `nuclear daemon stop`.
2. Run `nuclear doctor`.
3. Run `nuclear plugin doctor`.
4. Inspect recent logs with `nuclear logs` or the dashboard.
5. Restart with `nuclear daemon start`.

If a plugin is flagged for integrity drift:

1. Treat the installed package as modified.
2. Reinstall or update from the intended source.
3. Re-run `nuclear plugin doctor`.

## Auth Repair

If provider auth stops working:

1. Run `nuclear doctor`.
2. Re-run the relevant `nuclear login ...` or provider setup flow.
3. Confirm aliases still point at the intended provider and model.
4. Retry from the CLI or dashboard.

If the dashboard session becomes unusable, reconnect with a fresh dashboard session or daemon token.
