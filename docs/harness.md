# Harness

The release harness is organized into explicit lanes.

- `runtime-cert`: installer smoke, support-bundle smoke, daemon lifecycle, and operator-surface certification
- `coding-deterministic`: blocking fixture-repo coding evaluation with scripted provider turns
- `coding-reference`: fixture-repo coding evaluation using the configured `main` alias or an explicit provider profile
- `analysis-smoke`: supplemental structured-output and read-style smoke
- `soak`: long-running operational verification

## Entry Points

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-harness.ps1 -Lane runtime-cert
```

Linux:

```bash
./scripts/run-harness.sh --lane runtime-cert
```

Top-level release wrappers:

- `scripts/verify-workspace.*`
- `scripts/verify-ga.*`
- `scripts/finalize-release.*`

## Task Sources

- analysis smoke tasks: `harness/tasks/analysis-smoke/tasks.jsonl`
- coding tasks: `harness/tasks/coding/tasks.json`
- coding task schema: `harness/schemas/coding-task.schema.json`
- provider profile schema: `harness/schemas/provider-profile.schema.json`

Coding tasks are manifest-driven. They define:

- fixture repo
- operator prompt
- setup and success commands
- allowed and forbidden change boundaries
- tool and duration budgets
- final response assertions

## Fixture Behavior

The harness always copies fixtures into a scratch workspace, initializes a local git repository there, and evaluates the agent in that isolated copy. It never mutates the main repo during coding evaluation.

Current fixture coverage includes:

- bug fix
- failing-test repair
- bounded refactor
- config or migration repair
- docs and code consistency repair

## Provider Profiles

`coding-reference` uses the configured `main` alias by default. It can be overridden with:

- `--profile`
- `--alias`
- `--provider-id`
- `--model`
- `--provider-kind`
- `--base-url`
- `--api-key-env`

Precedence:

1. CLI flags
2. profile file
3. configured `main` alias

## Release Integration

`verify-workspace`:

- source and dependency gates
- release build
- fast runtime smoke

`verify-ga`:

- `verify-workspace`
- full `runtime-cert`
- strict clippy
- Playwright E2E
- blocking `coding-deterministic`

`finalize-release`:

- `verify-ga`
- packaging, signing, SBOM, provenance, and release record generation
- optional `coding-reference`
- optional `soak`
