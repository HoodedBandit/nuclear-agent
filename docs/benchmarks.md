# Benchmarks

The repo includes two benchmark layers:

- `benchmarks/coding-smoke/tasks.jsonl`: fast smoke coverage for CI and local verification
- `benchmarks/release-eval/tasks.jsonl`: broader prerelease coverage for coding-agent quality and operator-surface regression checks

## Inputs

Task files are JSONL. Each line must contain:

- `id`: stable task name
- `description`: optional label
- `category`: optional benchmark category label
- `tags`: optional string tags
- `command`: array of CLI arguments passed after the binary name
- `cwd`: optional working directory relative to the repo root or absolute path
- `expected_exit_code`: optional expected exit code, default `0`

Example:

```json
{"id":"workspace-summary","description":"Summarize workspace layout","command":["exec","--ephemeral","Inspect Cargo.toml and README.md, then summarize the workspace crates in five bullets."]}
```

## Running

Windows smoke run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-bench.ps1 -TaskFile .\benchmarks\coding-smoke\tasks.jsonl
```

Linux smoke run:

```bash
./scripts/run-bench.sh ./benchmarks/coding-smoke/tasks.jsonl
```

Windows prerelease run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-bench.ps1 -TaskFile .\benchmarks\release-eval\tasks.jsonl
```

Linux prerelease run:

```bash
./scripts/run-bench.sh ./benchmarks/release-eval/tasks.jsonl
```

Both scripts write results under `target/benchmarks/<timestamp>/`.

Each task gets:

- `request.json`
- `stdout.txt`
- `stderr.txt`
- `result.json`
- `stdout.json` when stdout is valid JSON
- `structured_output.json` when stdout includes `structured_output_json`

The run root also includes:

- `summary.json`: machine-readable run summary with pass/fail, duration, timestamps, artifact paths, and extracted provider/model/session metadata when available
- `summary.md`: human-readable run overview

Per-task results now include:

- pass/fail against `expected_exit_code`
- start/end timestamps
- stdout/stderr byte counts
- parsed metadata such as `provider_id`, `model`, `session_id`, and `tool_event_count` when stdout is a `RunTaskResponse` JSON object

## Expectations

- Build the CLI first so the benchmark script can invoke the binary directly.
- Use a configured local profile and provider state that can satisfy the requested commands.
- Treat `coding-smoke` as the CI-safe benchmark layer.
- Treat `release-eval` as the prerelease benchmark layer for deeper repo understanding, structured output, review, patch-planning, and tool-use checks.
- Compare `summary.json` and `summary.md` across runs instead of relying on anecdotal quality impressions.
