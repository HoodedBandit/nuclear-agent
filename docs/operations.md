# Operations

## Health Checks

Start with these checks before changing config:

```powershell
target\debug\nuclear.exe doctor
target\debug\nuclear.exe daemon status
target\debug\nuclear.exe plugin doctor
```

If the daemon is running, the dashboard also exposes live status, log, and plugin doctor views at `/dashboard`.

## Update Flow

Recommended update order:

1. Stop background autonomy or mission work.
2. Run `nuclear doctor`.
3. Update the binary or rebuild the workspace.
4. Run `nuclear plugin doctor`.
5. Refresh managed plugins with `nuclear plugin update <id>` when their recorded source changed.
6. Restart the daemon and re-run `nuclear doctor`.

## Rollback

This repo does not yet ship a dedicated rollback command. The practical rollback path today is:

1. Restore the previous binary or previous git revision.
2. Restart the daemon with that binary.
3. Re-run `nuclear plugin doctor`.
4. Reinstall or update any plugin package whose compatibility or integrity no longer matches the restored host.

Because plugin installs are copied into the daemon-managed data directory, restoring source files alone does not roll back an already installed plugin. Use `plugin update` or `plugin remove` plus `plugin install`.

## Recovery

If the daemon becomes unhealthy:

1. Run `nuclear daemon stop`.
2. Run `nuclear doctor`.
3. Run `nuclear plugin doctor`.
4. Inspect recent logs from the dashboard or `nuclear logs`.
5. Restart with `nuclear daemon start`.

If a plugin is flagged for integrity drift, treat the installed package as modified:

1. Compare the source reference and the installed package.
2. Reinstall from the intended source with `nuclear plugin update <id>` or `nuclear plugin install <source> --trust`.
3. Re-run `nuclear plugin doctor`.

Marketplace-backed installs resolve through `config/plugin-marketplace.json` by default, or the `AGENT_PLUGIN_MARKETPLACE_INDEX` override.

## Auth Repair

If a provider login stops working:

1. Run `nuclear doctor` and read the provider-specific error.
2. Re-run the appropriate `nuclear login ...` flow or re-add the provider credentials.
3. Verify aliases still point at the expected provider and model.
4. Retry the request from the CLI or dashboard.

If a dashboard session gets stuck, clear the browser session and reconnect with the daemon token.

## Reliability Smoke

Before shipping a build, run the repo and control-plane smoke checks:

```powershell
target\debug\nuclear.exe repo inspect .
powershell -ExecutionPolicy Bypass -File .\scripts\run-soak.ps1 -Token "<daemon-token>" -Workspace .
```
