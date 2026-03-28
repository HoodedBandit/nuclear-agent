# Release Checklist

Use this checklist before cutting a release or shipping a packaged build.

## 1. Baseline

- Ensure the worktree is clean: `git status --short`
- Confirm the canonical CLI name is `nuclear` and any remaining `autism` references are compatibility-only
- Review the compatibility surface before release notes are written: installer behavior, launcher aliases, persisted state, plugin protocol, and control-plane routes

## 2. Verification

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-workspace.ps1
npm run test:e2e
```

Linux:

```bash
./scripts/verify-workspace.sh
```

Expected coverage:

- Rust workspace check, test, and release build
- installer smoke for fresh install and legacy upgrade
- benchmark smoke artifact validation
- dependency duplicate/audit/deny/outdated checks when the optional cargo tools are installed
- dashboard Playwright E2E in CI or local prerelease verification

## 3. Benchmarks

Run the prerelease eval suite and keep the emitted `summary.json` and `summary.md` with the release notes or internal release record.

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-bench.ps1 -TaskFile .\benchmarks\release-eval\tasks.jsonl
```

Linux:

```bash
./scripts/run-bench.sh ./benchmarks/release-eval/tasks.jsonl
```

Review:

- pass/fail count
- structured output artifacts
- provider/model/session metadata
- any unexpected latency or tool-use drift

## 4. Product Surface Audit

- Smoke the installer output on the target platforms
- Confirm the dashboard boots, chat works, and session/transcript actions still behave correctly
- Confirm docs and examples use `nuclear` as the canonical name
- Confirm packaged installs still lay down the legacy `autism` launcher only as compatibility

## 5. Ship Readiness

- Record the release commit SHA
- Summarize compatibility notes in the release notes
- Call out any intentionally deferred debt or residual risk explicitly
