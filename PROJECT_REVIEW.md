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

- `nuclear memory profile`
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

- `nuclear mission add "Watch repo" --watch src`
- `nuclear mission resume <id> --watch src`
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
| Release build | `cargo build --release --bin nuclear --bin autism` | Pass |
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

---

## Changes Implemented — 2026-03-13 (Phases 1–4)

### Phase 1: Bug Fixes

**1a. OAuth refresh race condition** — Added per-account async mutex so concurrent token refreshes for the same account serialize instead of racing.
- Files: `crates/agent-providers/src/lib.rs`, `crates/agent-providers/Cargo.toml`

**1b. Windows write_atomic locked files** — `write_atomic` now retries on `ERROR_SHARING_VIOLATION` (32) and `ERROR_LOCK_VIOLATION` (33) with exponential backoff (50ms–800ms, 5 retries).
- Files: `crates/agent-storage/src/lib.rs`

**1c. File watch content hashing** — Replaced metadata-only fingerprinting with content hashing. Files <1MB use full content hash; files >1MB hash first 4KB + last 4KB + size.
- Files: `crates/agent-daemon/src/missions.rs`

**1d. Provider rate limiting** — Token bucket `ProviderRateLimiter` (60 RPM default per provider) acquired before each `run_prompt` call.
- Files: `crates/agent-daemon/src/lib.rs`, `crates/agent-daemon/src/runtime.rs`

### Phase 2: Structural Cleanup

Deleted `_recovery_quarantine/` (260MB dead recovery duplicates) and `dist/` (1.5GB old build bundles). Both were already gitignored with no source references.

### Phase 3: Memory & Learning Foundation

**3a. Semantic memory search — 3-tier pipeline:**
- Tier 1: `build_expanded_fts_query()` with English suffix stemmer (30+ suffix rules) for improved FTS recall
- Tier 2: `fuzzy_memory_search()` LIKE-based fallback when FTS returns < half the limit
- Tier 3: Embedding-based vector search — `memory_embeddings` table (BLOB vectors), cosine similarity, auto-embeds on upsert via `maybe_compute_embedding()`, supplements FTS via `embedding_search()`
- Pipeline order: stemmed FTS → fuzzy LIKE fallback → embedding cosine similarity
- Files: `crates/agent-core/src/lib.rs` (EmbeddingConfig), `crates/agent-providers/src/lib.rs` (compute_embedding), `crates/agent-storage/src/lib.rs` (memory_embeddings table, search functions), `crates/agent-daemon/src/memory.rs` (pipeline orchestration)

**3b. Evolve diff review enforcement:**
- `diff_review_required: bool` on `EvolveConfig` (default true)
- `diff_summary` field on `MissionDirective` and `EVOLVE_DIRECTIVE_SCHEMA`
- `handle_evolve_cycle` fails if files were mutated but no `diff_summary` provided
- `build_evolve_prompt` instructs agent to run `git diff` and include review
- Files: `crates/agent-core/src/lib.rs`, `crates/agent-daemon/src/missions.rs`

### Phase 4: Gmail Connector

Complete Gmail connector via Gmail REST API + OAuth2 Bearer tokens:
- Polling: `GET /messages?q=is:unread` → `GET /messages/{id}` for detail
- Sending: `POST /messages/send` with base64url-encoded RFC 2822 message
- Pairing approval workflow for unknown senders
- Admin CRUD, approval routing, connector orchestration
- Files:
  - `crates/agent-core/src/lib.rs` (Gmail types, ConnectorKind::Gmail, WakeTrigger::Gmail, GmailConnectorConfig)
  - `crates/agent-daemon/src/connectors/gmail.rs` (NEW — full implementation)
  - `crates/agent-daemon/src/connectors.rs` (Gmail routing, polling integration)
  - `crates/agent-daemon/src/connectors/admin.rs` (Gmail CRUD)
  - `crates/agent-daemon/src/connectors/approvals.rs` (Gmail approval branch)
  - `crates/agent-daemon/src/routes.rs` (Gmail routes /v1/gmail/*)
  - `crates/agent-daemon/src/control.rs` (gmail_connectors in DaemonStatus)
  - `crates/agent-daemon/Cargo.toml` (base64 dependency)

### Phase 5: Intelligence Layer

**5a. Proactive learning — pattern tracking:**
- New `PatternType` enum (ToolSequence, ErrorRecovery, PreferredWorkflow, AvoidedAction) and `UsagePattern` struct in agent-core
- New `usage_patterns` SQLite table with CRUD operations in agent-storage
- New `crates/agent-daemon/src/patterns.rs` — detects tool sequences, error-recovery retries, and preferred workflows from tool events
- Pattern detection wired into `learn_from_interaction()` — auto-records patterns after each interaction with 2+ tool events
- Files: `crates/agent-core/src/lib.rs`, `crates/agent-storage/src/lib.rs`, `crates/agent-daemon/src/patterns.rs` (NEW), `crates/agent-daemon/src/lib.rs`, `crates/agent-daemon/src/memory.rs`

**5b. Adaptive behavior — preference injection:**
- `load_pattern_guidance()` surfaces top recurring patterns (frequency ≥ 2, confidence ≥ 40) as system prompt context
- Injected into `execute_task_request()` as an additional system message after memory context
- Agent sees observed patterns from prior interactions to inform its approach
- Files: `crates/agent-daemon/src/patterns.rs`, `crates/agent-daemon/src/runtime.rs`

**5c. Smarter evolve — workspace improvement signals:**
- `gather_evolve_signals()` scans workspace before each evolve cycle for: TODO/FIXME/HACK counts, large files (>800 lines), clippy warnings
- Signals injected into the evolve prompt to guide improvement target selection
- Files: `crates/agent-daemon/src/missions.rs`

### Phase 6: Dashboard Feature Parity

Full web dashboard rewrite achieving CLI feature parity:

**New sections added:**
- **Providers & Aliases** — List providers with kind/auth/keychain info, list aliases, add alias form
- **Memory Tools** — Search form (POST /v1/memory/search), create memory form (kind/scope/subject/content), forget/delete buttons on memory cards
- **Permissions & Trust** — Permission preset selector (Suggest/AutoEdit/FullAuto), trust policy toggles (shell/network/full_disk/self_edit)
- **Add Connector** — Type dropdown (telegram/discord/slack/signal/home-assistant/webhook/inbox/gmail) with dynamic type-specific fields, delete buttons on existing connectors
- **Gmail Connectors** — Added to connector fetch, display, toggle, poll, delete
- **Run Task** — Prompt execution form with alias, model override, thinking level selector, response display
- **Sessions** — Session list table with expandable detail view for messages
- **Logs** — Scrollable daemon log viewer (GET /v1/logs)
- **MCP Servers** — List with delete + add form (id, name, command, args, enabled)
- **Daemon Config** — Persistence mode toggle (on_demand/always_on), auto-start toggle

**Infrastructure changes:**
- `apiDelete()` helper added
- `refreshDashboard()` now fetches 24 endpoints in parallel (was 16) with graceful fallbacks
- Dynamic connector form fields via `connectorTypeFields()`/`updateConnectorAddFields()`
- 12 new action handlers in `bindActions()`

Files:
- `crates/agent-daemon/static/dashboard.html` (expanded with 9 new panel sections, 5 new nav links)
- `crates/agent-daemon/static/dashboard.js` (expanded with 8 new render functions, 12 action handlers)
- `crates/agent-daemon/static/dashboard.css` (new panel span rules, select element styling)

### Verification

All 171 tests pass, clippy clean, zero warnings after all phases.

## Recommended Next Steps

1. Split `crates/agent-daemon/src/lib.rs` into smaller modules before layering more autonomy features onto it.
2. Expand the learning pipeline to consolidate assistant conclusions and tool outcomes into reviewable long-term memory records.
3. Add end-to-end integration tests for connector workflows and dashboard API coverage.
