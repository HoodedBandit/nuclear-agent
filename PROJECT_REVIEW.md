# Project Review

Date: 2026-03-09

## Scope

Shipping workspace only:

- `crates/agent-cli`
- `crates/agent-core`
- `crates/agent-daemon`
- `crates/agent-policy`
- `crates/agent-providers`
- `crates/agent-storage`

Reference trees such as `codex-main`, `claude-code-main`, `hermes-agent-main`, and `dist` were not treated as production code.

## Changes Implemented In This Pass

### 1. Structured mission control

Background mission execution no longer depends only on a brittle `[AUTOPILOT]` text block. `agent-daemon` now requests a typed JSON control object through structured output and still keeps the legacy parser as a fallback.

Files:

- `crates/agent-daemon/src/lib.rs`

### 2. Memory learning from execution artifacts

The daemon memory pipeline no longer learns only from the user prompt. It now learns from:

- the user prompt
- the final assistant reply
- successful tool transcript messages

This also adds basic provenance tags and message-level provenance for tool-derived memories.

Files:

- `crates/agent-daemon/src/lib.rs`

### 3. Dependency hardening

`Cargo.lock` was updated to move `quinn-proto` to `0.11.14`, eliminating the known `quinn-proto` DoS advisory that was previously reachable through `reqwest`.

Files:

- `Cargo.lock`

### 4. Claude browser auth bootstrap is now internal

Claude browser login no longer shells out to `claude auth login/status`. The CLI now uses the packaged Claude Code OAuth constants and flow shape directly: PKCE browser auth, token exchange, then managed API-key minting through Anthropic's `claude_cli` endpoints. Existing `~/.claude.json` credentials are still reused as a convenience fallback, but they are no longer required.

Files:

- `crates/agent-cli/src/main.rs`

### 5. Stable keyring and operator surfaces for learned state

The workspace no longer depends on `keyring 4.0.0-rc.3` or the `db-keystore -> turso -> paste` chain. `agent-providers` now uses stable `keyring 3.6.3` with native backends per platform target. This removes the prior `cargo deny` advisory blocker.

The CLI/TUI also gained first-class operator surfaces for resident profile memory and learned skill drafts:

- `autism memory profile`
- `/profile`
- `/skills [drafts|published|rejected]`
- `/skills publish <draft-id>`
- `/skills reject <draft-id>`
- TUI settings entries for `Resident profile` and `Learned skills`

Files:

- `Cargo.toml`
- `Cargo.lock`
- `crates/agent-providers/Cargo.toml`
- `crates/agent-providers/src/lib.rs`
- `crates/agent-daemon/src/lib.rs`
- `crates/agent-cli/src/main.rs`
- `crates/agent-cli/src/tui/app.rs`
- `crates/agent-cli/src/tui/render.rs`

### 6. Filesystem-triggered resident missions

The daemon can now wake missions on workspace/file changes instead of only timers. Missions carry watch metadata (`watch_path`, `watch_recursive`, `watch_fingerprint`), the autopilot loop detects file changes before dispatch, and both CLI and TUI can create watched missions directly:

- `autism mission add "Watch repo" --watch src`
- `autism mission resume <id> --watch src`
- `/watch <path> <title>`

Files:

- `crates/agent-core/src/lib.rs`
- `crates/agent-storage/src/lib.rs`
- `crates/agent-daemon/src/lib.rs`
- `crates/agent-cli/src/main.rs`
- `crates/agent-cli/src/tui/app.rs`
- `crates/agent-cli/src/tui/render.rs`

## Findings

### Medium

- `agent-daemon/src/lib.rs` is still too large and carries too many responsibilities in one module. It remains patchable, but not comfortably so. The next refactor should split it into focused modules for:
  - mission runner
  - memory pipeline
  - delegation/routing
  - HTTP handlers
  - policy enforcement

### Low

- `cargo tree --workspace --duplicates` still shows duplicate transitive families such as `foldhash`, `getrandom`, `hashbrown`, and Windows target crates. These are mostly ecosystem/version-split duplicates, not immediate correctness bugs. They should be watched, but they are not the main blocker.

## Verification Matrix

| Surface | Method | Result |
| --- | --- | --- |
| Daemon unit tests | `cargo test -p agent-daemon` | Pass |
| Workspace tests | `cargo test --workspace` | Pass |
| Workspace compile | `cargo check --workspace` | Pass |
| Release build | `cargo build --release --bin autism` | Pass |
| Security advisories | `cargo audit` | Pass |
| Dependency policy | `cargo deny check advisories licenses bans` | Pass; duplicate-crate warnings remain informational |
| Duplicate dependency review | `cargo tree --workspace --duplicates` | Reviewed; duplicates are mostly transitive/version-split |

## New Regression Coverage Added

- structured JSON mission directive parsing
- legacy mission directive fallback parsing
- tool/system-derived memory extraction with provenance
- filesystem watch missions prime and detect changes
- watched mission prompts include filesystem watch context

Files:

- `crates/agent-daemon/src/lib.rs`

## Recommended Next Steps

1. Split `crates/agent-daemon/src/lib.rs` into smaller modules before layering more autonomy features onto it.
2. Expand the learning pipeline again so it consolidates assistant conclusions and tool outcomes into reviewable long-term memory records, not just heuristic phrase matches.
3. Build the next resident-runtime layer: more connector/event sources and richer TUI management for learned skills/profile/schedules.
