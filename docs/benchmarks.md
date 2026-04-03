# Harness Compatibility

The old benchmark entrypoints still exist for one release cycle:

- `scripts/run-bench.ps1`
- `scripts/run-bench.sh`
- `scripts/run_bench.py`

They now delegate to the canonical harness runner in analysis mode and are no longer the authoritative coding-agent gate.

Use [harness.md](harness.md) for the current harness lanes, task/profile formats, and release-gate usage.
