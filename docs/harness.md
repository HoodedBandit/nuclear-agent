# Harness

The release harness is organized into explicit lanes:

- `runtime-cert`: installer smoke, support-bundle smoke, daemon lifecycle, and operator-surface certification
- `coding-deterministic`: blocking fixture-repo coding evaluation with scripted provider turns
- `coding-reference`: fixture-repo coding evaluation using the configured `main` alias by default or an explicit provider profile
- `analysis-smoke`: supplemental read/review/structured-output smoke
- `soak`: long-running operational soak

## Canonical entrypoints

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-harness.ps1 -Lane runtime-cert
```

Linux:

```bash
./scripts/run-harness.sh --lane runtime-cert
```

Top-level release entrypoints:

- `scripts/verify-workspace.ps1` / `scripts/verify-workspace.sh`
- `scripts/verify-ga.ps1` / `scripts/verify-ga.sh`
- `scripts/finalize-release.ps1` / `scripts/finalize-release.sh`

## Task files and schemas

- analysis smoke tasks: `harness/tasks/analysis-smoke/tasks.jsonl`
- coding tasks: `harness/tasks/coding/tasks.json`
- coding task schema: `harness/schemas/coding-task.schema.json`
- provider profile schema: `harness/schemas/provider-profile.schema.json`

Coding tasks are manifest-driven. Required fields include:

- task id
- fixture path
- suite label
- prompt
- setup and success commands
- allowed and forbidden change globs
- duration and tool-call budgets
- required tools
- final response assertions

Optional fields include:

- precondition commands
- expected failing commands before the run
- post-run assertions
- expected changed paths
- cleanup commands
- deterministic script path

## Fixture behavior

The coding harness always copies fixtures into a scratch workspace, initializes a local git repository, and runs the task there. It never mutates the main repo during evaluation.

Current fixture coverage includes:

- bug fix
- failing-test repair with recovery from an insufficient first edit
- bounded refactor
- config migration repair
- docs and code consistency repair

## Provider profiles

`coding-reference` uses the configured `main` alias by default. You can override it with:

- `--profile <json>`
- `--alias`
- `--provider-id`
- `--model`
- `--provider-kind`
- `--base-url`
- `--api-key-env`

Precedence is:

- CLI flags
- profile file
- configured `main` alias

## Release integration

`verify-workspace`:

- source/build/test
- release build
- fast runtime smoke through `runtime-cert` filtered to installer and support-bundle checks

`verify-ga`:

- `verify-workspace`
- full `runtime-cert`
- strict clippy
- Playwright E2E
- blocking `coding-deterministic`

`finalize-release`:

- `verify-ga`
- package/sign/SBOM/provenance
- optional `coding-reference`
- optional `soak`
- production release record
