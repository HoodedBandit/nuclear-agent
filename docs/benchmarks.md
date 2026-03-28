# Benchmarks

The repo now includes a lightweight benchmark harness for repeatable coding-agent smoke runs.

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

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-bench.ps1 -TaskFile .\benchmarks\coding-smoke\tasks.jsonl
```

Linux:

```bash
./scripts/run-bench.sh ./benchmarks/coding-smoke/tasks.jsonl
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
- Treat this harness as a repeatable operator and product benchmark surface, not a CI gate.
