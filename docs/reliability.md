# Reliability

## Goal

Use this repo to validate that the daemon, dashboard bootstrap path, and workspace-inspection API stay stable under repeated operator traffic before cutting a build.

## Fast Checks

```powershell
target\debug\nuclear.exe doctor
target\debug\nuclear.exe plugin doctor
target\debug\nuclear.exe repo inspect .
```

## Soak Harness

The soak harness repeatedly exercises:

- `GET /v1/status`
- `GET /v1/dashboard/bootstrap`
- `POST /v1/workspace/inspect`

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-soak.ps1 -Token "<daemon-token>" -Workspace .
```

Linux:

```bash
./scripts/run-soak.sh "<daemon-token>" "http://127.0.0.1:42690" 30 1000 .
```

Environment variables also work:

- `AGENT_TOKEN`
- `AGENT_BASE_URL`
- `AGENT_WORKSPACE_PATH`
- `AGENT_SOAK_OUTPUT_ROOT`

Each soak run now writes artifacts under `target/soak/<timestamp>/` by default:

- `samples.jsonl`
- `summary.json`
- `summary.md`

## What To Watch

- Average and slowest iteration time
- Unexpected growth in `dirty_files`, session count, or plugin count
- Any failures resolving the workspace scan path
- Dashboard or doctor failures after repeated refresh/update cycles

## Plugin Review Stability

Plugin trust review is bound to the installed package hash. After `plugin update`, expect:

1. `plugin doctor` to report that review is needed again if bytes changed.
2. Runtime projection to stay blocked until the plugin is re-reviewed.
3. Permission grants to remain clamped to the manifest-declared capabilities.
