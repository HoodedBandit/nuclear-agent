# Release Checklist

Use this checklist before cutting a release or shipping a packaged build. For beta releases, treat
**Windows as the blocking platform** and keep Linux compatibility intact without making Linux a
manual signoff gate.

## Phase 1. Core Runtime and Platform Hardening

- Ensure the worktree is clean: `git status --short`
- Confirm the canonical CLI name is `nuclear` and any remaining `autism` references are compatibility-only
- Review the compatibility surface before release notes are written: installer behavior, launcher aliases, persisted state, plugin protocol, and control-plane routes
- Verify the shipped Windows path works end to end: install, onboarding, daemon lifecycle, provider login, chat/run, session resume/fork/compact, shutdown, reset, and recovery
- Resolve blocker-level Windows issues in provider auth, plugin install/update, dashboard auth, daemon restart, and persisted state handling before advancing

## Phase 2. Operator Surface Hardening

Beta signoff assumes every operator-facing surface remains in scope:

- dashboard and session workflows
- providers and aliases
- plugins and plugin doctor
- connectors, approvals, and polling/sending flows
- memory, missions, autonomy, evolve, permissions, delegation, MCP/apps, logs, and doctor

For each surface, require:

- one documented happy path
- one clear failure/recovery path
- one persistence or restart validation

Provider certification must cover every listed provider path:

- OpenAI-compatible
- ChatGPT Codex
- Anthropic
- Moonshot
- OpenRouter
- Venice
- Ollama

For each provider, sign off on credential setup, model listing, prompt execution, and failure messaging when auth or model access is invalid.

## Phase 3. Beta Verification and Signoff

### Core verification

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-beta.ps1
```

Linux compatibility:

```bash
./scripts/verify-beta.sh --skip-e2e
```

`verify-beta` expands the baseline workspace verification to include:

- Rust workspace check, test, and release build
- installer smoke for fresh install and legacy upgrade
- packaged installer smoke for fresh install and legacy upgrade
- isolated Phase 1 runtime smoke for daemon lifecycle, prompt execution, session recovery, dashboard launch auth, restart persistence, and reset recovery
- isolated Phase 2 operator surface smoke for provider and alias management, plugin doctor lifecycle, inbox connector recovery, memory and mission workflows, MCP/apps, autopilot status/config, and restart persistence
- isolated Phase 2 certification matrix for every shipped provider path, delegation controls, webhook delivery, Telegram/Discord/Slack/Signal approvals and sends, Home Assistant polling and service restrictions, Gmail approval and send flows, Brave tool routing, dashboard bootstrap counts, and restart persistence
- benchmark smoke artifact validation
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- dependency duplicate/audit/deny/outdated checks when the optional cargo tools are installed
- dashboard Playwright E2E
- prerelease `release-eval` benchmarks

### Additional release records

Windows final signoff:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-phase3.ps1 -Token "<daemon-token>" -Workspace .
```

Linux compatibility signoff:

```bash
./scripts/verify-phase3.sh --skip-e2e --skip-soak
```

`verify-phase3` runs `verify-beta`, packages the release bundle, optionally runs the soak harness, and writes a timestamped release record under `target/release-records/`.

Run the soak harness and keep the emitted soak and benchmark artifacts with the release record.

Windows soak:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-soak.ps1 -Token "<daemon-token>" -Workspace .
```

Review benchmark and soak artifacts for:

- pass/fail count
- structured output artifacts
- provider/model/session metadata
- unexpected latency or tool-use drift
- repeated operator traffic regressions in status, bootstrap, and workspace inspection

### Final certification

- Smoke the installer output on Windows and confirm packaged installs keep the legacy `autism` launcher as compatibility only
- Confirm docs and examples use `nuclear` as the canonical name
- Confirm the packaged bundle is named `nuclear-<version>-windows-<arch>-full`
- Record the release commit SHA
- Summarize compatibility notes in [`docs/beta-release-notes.md`](beta-release-notes.md)
- Call out any intentionally deferred debt or residual risk explicitly
