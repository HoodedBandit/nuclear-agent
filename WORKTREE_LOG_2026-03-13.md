## Worktree Log

Date: 2026-03-13

Purpose: Record the current uncommitted workspace edits as the accepted in-progress baseline after prior cleanup work.

### Current Edit Set

The active worktree includes daemon, storage, dashboard, and shared-model changes that were already present in the repository before this log entry was added.

High-level areas covered by the current edits:

- Gmail connector support across core types, daemon routes, connector admin, polling, approvals, and send paths
- Usage pattern tracking and prompt guidance plumbing
- Memory and evolve-path improvements
- Dashboard expansion toward closer feature parity with the CLI
- GUI browser sign-in for ChatGPT Codex and Claude providers, backed by daemon callback/state handling
- Supporting schema, dependency, and review-note updates

Primary touched files:

- `Cargo.lock`
- `PROJECT_REVIEW.md`
- `crates/agent-core/src/lib.rs`
- `crates/agent-daemon/Cargo.toml`
- `crates/agent-daemon/src/connectors.rs`
- `crates/agent-daemon/src/connectors/admin.rs`
- `crates/agent-daemon/src/connectors/approvals.rs`
- `crates/agent-daemon/src/control.rs`
- `crates/agent-daemon/src/auth.rs` (new)
- `crates/agent-daemon/src/lib.rs`
- `crates/agent-daemon/src/memory.rs`
- `crates/agent-daemon/src/missions.rs`
- `crates/agent-daemon/src/routes.rs`
- `crates/agent-daemon/src/runtime.rs`
- `crates/agent-daemon/static/dashboard.css`
- `crates/agent-daemon/static/dashboard.html`
- `crates/agent-daemon/static/dashboard.js`
- `crates/agent-storage/src/lib.rs`
- `crates/agent-daemon/src/connectors/gmail.rs` (new)
- `crates/agent-daemon/src/patterns.rs` (new)

Diff summary at time of logging:

- 17 modified tracked files
- 2 new untracked source files
- About 1660 insertions and 56 deletions in tracked files

### Verification

Verified successfully on 2026-03-13 against the current worktree:

- `cargo check --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo build --release --bin autism`
- `node --check crates/agent-daemon/static/dashboard.js`

### Test Package

A Windows test bundle was assembled from this worktree at:

- `dist-test/autism-cli-bundle-windows-x64`

The packaged installer was smoke-tested into:

- `dist-test/install-smoke`

### Notes

- This log records the current edits; it does not convert them into a clean commit.
- `dist-test/` is gitignored and was used only for build/package verification.

### Security Progress

Additional work completed on 2026-03-13 after the initial baseline:

- Dashboard auth hardening:
  - removed persistent bearer-token storage from browser local state
  - added session-scoped cookie-backed dashboard auth so the GUI no longer keeps the daemon token in long-lived JS storage
  - scrubbed `?token=` from the dashboard URL after bootstrap
- Browser auth hardening:
  - browser sign-in session state no longer retains raw provider secret payloads after completion
  - completed and failed browser-auth sessions are pruned after a short terminal TTL
- Tool/session secrecy hardening:
  - secret-bearing connector tool arguments are redacted before tool-event summaries and session persistence
  - persisted provider payload snapshots are dropped for assistant turns that contain sensitive tool calls
- Connector error hardening:
  - Telegram request errors now redact token-bearing URL segments before surfacing
- Browser response hardening:
  - dashboard and auth popup responses now set CSP, `Referrer-Policy`, and `X-Content-Type-Options`

Security verification re-run successfully after these changes:

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `node --check crates/agent-daemon/static/dashboard.js`
