# Secrets and Auth Leakage Audit

Date: 2026-03-13

Scope:
- `crates/agent-daemon/static/dashboard.js`
- `crates/agent-daemon/src/auth.rs`
- `crates/agent-daemon/src/control.rs`
- `crates/agent-daemon/src/runtime.rs`
- `crates/agent-daemon/src/connectors/admin.rs`
- `crates/agent-daemon/src/connectors/telegram.rs`
- `crates/agent-daemon/src/tools/connector_tools/messaging.rs`
- `crates/agent-daemon/src/tools/connector_tools/home_assistant_tools.rs`
- `crates/agent-daemon/src/tools/connector_tools/brave_tools.rs`
- `crates/agent-storage/src/lib.rs`
- `crates/agent-core/src/lib.rs`

Objective:
- Identify practical leakage paths for API keys, OAuth tokens, bearer tokens, and adjacent auth/session material.
- Produce a prioritized remediation backlog with minimal, safe fixes.

Method:
- Static code review only.
- No live credentials used.
- Focused on source-to-sink handling, browser storage, daemon memory retention, logging, error propagation, and persistence.

## Executive Summary

The highest-risk issues are not in keychain storage itself. The daemon generally stores provider and connector secrets in keychain-backed accounts instead of raw config, which is good. The real exposure is around secondary handling:

1. The dashboard bearer token is accepted from `?token=` and persisted in `localStorage`, which makes it easy to retain and expose through browser history and any same-origin script execution.
2. Browser OAuth sessions keep full `ProviderUpsertRequest` objects in daemon memory, including raw API keys or OAuth tokens, and completed sessions are not removed from the in-memory session map.
3. Secret-bearing connector admin tools accept raw secrets as ordinary tool arguments, then the runtime persists tool calls to session history and can echo tool arguments in repeated-tool summaries. That creates a direct credential-at-rest leak path.
4. Telegram bot tokens are embedded in request URLs. Several error paths format request errors directly, which can expose a full token-bearing URL if the HTTP stack includes the URL in the error string.

## Secret Lifecycle Map

### Dashboard bearer token
- Entry: query string `?token=...` or manual token input.
- Storage: browser `localStorage`.
- Use: sent as `Authorization: Bearer ...` to daemon API.
- Main risk: browser history retention, XSS exposure, shared machine persistence.

### Provider API keys and OAuth tokens
- Entry: provider upsert payloads and browser auth completion.
- Storage target: keychain-backed secret store via `store_api_key` / `store_oauth_token`.
- Secondary retention risk: browser auth session state keeps `ProviderUpsertRequest` in daemon memory after completion.

### Connector secrets
- Entry: connector admin routes and agent-facing connector config tools.
- Storage target: keychain-backed secret store via `store_connector_secret`.
- Secondary retention risk: tool-call arguments containing raw secrets are persisted to session history and may be echoed in summaries.

### Webhook tokens
- Entry: webhook connector create/update.
- Storage target: SHA-256 hash only.
- Main note: this path is materially safer than the others reviewed.

## Findings

### Critical 1: Secret-bearing tool arguments are persisted to session history and can be echoed back to users

Impact:
- Raw connector secrets can be written into SQLite session history and then reloaded into the dashboard/session views.
- Repeated tool-loop summaries can also include the raw argument string, making accidental disclosure visible to operators.
- This is a direct credential disclosure path for Telegram bot tokens, Discord bot tokens, Slack bot tokens, Home Assistant access tokens, and any similar secret passed through connector config tools.

Evidence:
- Secret-bearing connector tools accept secrets as ordinary arguments:
  - `bot_token` in `configure_telegram_connector`, `configure_discord_connector`, and `configure_slack_connector` at `crates/agent-daemon/src/tools/connector_tools/messaging.rs:184-204`, `crates/agent-daemon/src/tools/connector_tools/messaging.rs:343-365`, and `crates/agent-daemon/src/tools/connector_tools/messaging.rs:511-533`
  - `access_token` in `configure_home_assistant_connector` at `crates/agent-daemon/src/tools/connector_tools/home_assistant_tools.rs:285-318`
- Assistant tool calls are copied into transcript messages in `crates/agent-daemon/src/runtime.rs:452-456`
- Repeated tool summary includes `tool.arguments` in `crates/agent-daemon/src/runtime.rs:552-560`
- Session persistence serializes `message.tool_calls` into `tool_calls_json` in `crates/agent-storage/src/lib.rs:802-818` and `crates/agent-storage/src/lib.rs:868-889`
- The storage test explicitly proves tool-call metadata round-trips in `crates/agent-storage/src/lib.rs:2741-2802`

Likely exploit path:
- Operator pastes a secret into a chat flow that results in a connector config tool call.
- The model emits a tool call containing the secret in JSON arguments.
- The daemon persists that tool call verbatim to session history.
- The secret becomes recoverable from the session database and potentially visible in UI/session export flows.

Smallest safe fix:
- Do not persist raw tool arguments for designated secret-bearing tools.
- Add a `sensitive_args` or `redacted_arguments` concept at the tool-call model layer.
- Redact `tool.arguments` before:
  - repeated tool summaries
  - `SessionMessage.tool_calls`
  - any log/event surface
- For connector config tools, prefer admin API forms over agent tool calls for secrets.

Regression risk:
- Moderate. Session history, debugging, and any tooling that expects exact tool args will need an explicit redaction-aware path.

### High 2: Dashboard bearer token is stored in `localStorage` and accepted from `?token=` without URL scrubbing

Impact:
- Bearer token persists in browser storage across sessions and across daemon restarts on the same machine.
- Any same-origin XSS or malicious browser extension can read it.
- `?token=` in the URL can remain in browser history and can leak through screenshots, copy/paste, shell history, and local diagnostics.

Evidence:
- Token is written to `localStorage` in `crates/agent-daemon/static/dashboard.js:2182-2189`
- Token is read from `window.location.search` and `localStorage` in `crates/agent-daemon/static/dashboard.js:2192-2198`
- The dashboard code does not remove the token from the URL after reading it; no `history.replaceState` or equivalent is present in `crates/agent-daemon/static/dashboard.js`

Likely exploit path:
- Operator opens dashboard with `?token=...`
- Token is stored in browser history and in `localStorage`
- Any script execution on the page or local machine access can recover the daemon bearer token and call privileged API routes

Smallest safe fix:
- Stop accepting bearer tokens via query string by default.
- If URL bootstrap must remain temporarily supported, read it once and immediately scrub it with `history.replaceState`.
- Move token storage from `localStorage` to memory-only state or, if persistence is truly required, a tighter-scoped secure transport design rather than script-readable storage.
- Add explicit logout/clear-token behavior on tab close or inactivity if persistent browser auth remains necessary.

Regression risk:
- Low to moderate. Users relying on current persistent browser login behavior will notice the change.

### High 3: Browser auth sessions retain full provider secret material in daemon memory after completion

Impact:
- Completed or failed browser auth sessions can retain API keys or OAuth tokens in daemon memory longer than necessary.
- Long-lived daemon processes accumulate sensitive session material in the `browser_auth_sessions` map.
- Memory retention expands blast radius for diagnostics, crashes, future debug endpoints, or accidental in-process disclosure.

Evidence:
- `BrowserAuthSessionRecord` stores `provider_request: ProviderUpsertRequest` in `crates/agent-daemon/src/auth.rs:60-72`
- `ProviderUpsertRequest` includes `api_key` and `oauth_token` in `crates/agent-core/src/lib.rs:2005-2009`
- Browser auth start inserts the full request into the session map in `crates/agent-daemon/src/auth.rs:251-263`
- Completion updates the stored `provider_request` again in `crates/agent-daemon/src/auth.rs:785-794`
- Status reads the session back from the same in-memory map in `crates/agent-daemon/src/auth.rs:293-302` and returns metadata via `to_status_response` in `crates/agent-daemon/src/auth.rs:878-886`
- `expire_pending_session` clears PKCE state on timeout but does not remove completed sessions or scrub `provider_request` in `crates/agent-daemon/src/auth.rs:889-898`
- No `remove` call on `browser_auth_sessions` was found in `crates/agent-daemon/src`

Likely exploit path:
- Operator completes GUI OAuth flow
- Session remains addressable in daemon memory
- A future bug, dump, or introspection path exposes retained provider auth material

Smallest safe fix:
- Replace stored `ProviderUpsertRequest` with a redacted/minimal session record:
  - provider id
  - display name
  - alias metadata
  - auth kind
- After credentials are committed to keychain/config, delete the session from the map or overwrite secret-bearing fields with `None`.
- Add TTL-based scavenging for all terminal session states, not just pending sessions.

Regression risk:
- Low. The status API already returns only non-secret metadata.

### Medium 4: Telegram bot tokens are embedded in request URLs and error strings can leak those URLs

Impact:
- Telegram tokens appear directly in request paths, so any error string that includes the request URL can expose the token.
- The risk is amplified where request failures are formatted directly into user-facing or loggable strings.

Evidence:
- Telegram connector polling uses `https://api.telegram.org/bot{token}/getUpdates` in `crates/agent-daemon/src/connectors/telegram.rs:191-202`
- Telegram connector send uses `https://api.telegram.org/bot{token}/sendMessage` in `crates/agent-daemon/src/connectors/telegram.rs:237-250`
- Agent-facing Telegram helper uses `https://api.telegram.org/bot{bot_token}/getMe` in `crates/agent-daemon/src/tools/connector_tools/messaging.rs:939-943`
- Agent-facing Telegram send tool uses `https://api.telegram.org/bot{token}/sendMessage` in `crates/agent-daemon/src/tools/connector_tools/messaging.rs:1243-1248`

Why this matters:
- Telegram’s API shape forces the token into the URL path, so the practical control is redaction and careful error handling.
- `reqwest` and related error formatting can include the request URL depending on failure mode.

Smallest safe fix:
- Never include raw `reqwest::Error` text for Telegram requests in user-visible or loggable strings without redaction.
- Wrap Telegram request failures in fixed messages that do not include the URL.
- If detailed debug logging is ever needed, redact `/bot<token>/` before emission.

Regression risk:
- Low.

### Medium 5: Secret-bearing admin request types still expose raw secret fields at the API boundary

Impact:
- Provider and connector upsert routes correctly move secrets into keychain-backed storage, but the raw secret values still exist at the HTTP/API boundary and in deserialized request objects.
- This is expected for write endpoints, but it raises the bar for logging, tracing, panic handling, and generic request dumping.

Evidence:
- `ProviderUpsertRequest` contains `api_key` and `oauth_token` in `crates/agent-core/src/lib.rs:2005-2009`
- Connector upsert requests contain `bot_token`, `access_token`, `oauth_token`, and `api_key` in `crates/agent-core/src/lib.rs:2196-2210` and `crates/agent-core/src/lib.rs:2428-2438`
- The write paths correctly `take()` and store secrets in keychain-backed storage in `crates/agent-daemon/src/control.rs:179-205` and `crates/agent-daemon/src/connectors/admin.rs:9-15`, `crates/agent-daemon/src/connectors/admin.rs:728-740`, `crates/agent-daemon/src/connectors/admin.rs:823-835`

Assessment:
- This is not itself a leak. It is a risk multiplier if request logging, panic dumps, or generic middleware logging is added later.

Smallest safe fix:
- Mark these request types as sensitive in internal conventions and avoid `Debug` logging of whole payloads.
- Add a redaction helper for any future request logging or telemetry layer.
- Consider separate “secret replace” endpoints or form-data handling if the API surface continues to expand.

Regression risk:
- Low.

## Existing Controls That Look Good

These are worth preserving:

- Provider secrets are moved into keychain-backed storage and not returned in the provider response path in `crates/agent-daemon/src/control.rs:179-205`
- Connector secrets are stored through `store_connector_secret` and validated by keychain-account reference in `crates/agent-daemon/src/connectors/admin.rs:9-26`
- Webhook tokens are hashed with SHA-256 rather than stored raw in `crates/agent-daemon/src/connectors/admin.rs:45-48`
- Secret deletion paths exist for providers and connectors through `delete_secret(...)` in `crates/agent-daemon/src/control.rs:231-276` and multiple connector delete paths in `crates/agent-daemon/src/connectors/admin.rs`
- Browser auth status responses intentionally expose only metadata, not raw credential fields, in `crates/agent-core/src/lib.rs:2075-2083` and `crates/agent-daemon/src/auth.rs:878-886`

## Prioritized Remediation Backlog

### Critical

1. Redact secret-bearing tool arguments before they enter transcript/session persistence.
- Affected areas:
  - `crates/agent-daemon/src/runtime.rs`
  - `crates/agent-storage/src/lib.rs`
  - `crates/agent-daemon/src/tools/connector_tools/messaging.rs`
  - `crates/agent-daemon/src/tools/connector_tools/home_assistant_tools.rs`
- Suggested implementation:
  - add tool metadata for `sensitive_input_fields`
  - replace matching values with `[REDACTED]` before persistence and summaries
  - disable raw-argument round-trip for connector config tools

### High

2. Remove dashboard bearer token support from query strings and stop persisting it in `localStorage`.
- Affected area:
  - `crates/agent-daemon/static/dashboard.js`
- Suggested implementation:
  - immediate `history.replaceState` if temporary URL bootstrap remains
  - memory-only token storage
  - optional short-lived session cookie or one-time bootstrap flow later

3. Scrub or delete browser auth session records as soon as completion status is finalized.
- Affected area:
  - `crates/agent-daemon/src/auth.rs`
- Suggested implementation:
  - replace `ProviderUpsertRequest` in session state with a minimal redacted struct
  - delete completed/failed sessions after a short TTL
  - clear all secret-bearing fields immediately after keychain commit

### Medium

4. Redact Telegram request failures so token-bearing URLs can never appear in logs or API errors.
- Affected areas:
  - `crates/agent-daemon/src/connectors/telegram.rs`
  - `crates/agent-daemon/src/tools/connector_tools/messaging.rs`

5. Add a project-wide “no secret payload logging” rule for provider and connector upsert requests.
- Affected areas:
  - shared request types in `crates/agent-core/src/lib.rs`
  - future logging/telemetry middleware

## Runtime Verification Still Needed

The following controls were not provable from repo code alone and should be verified in a live deployment:

- CSP and other browser hardening headers for the dashboard
- Referrer policy and cache-control headers on auth and dashboard responses
- Whether any reverse proxy or launcher logs full dashboard URLs containing `?token=...`
- Crash dump and panic-report behavior in production builds

## Recommended Next Step

Fix the Critical and High findings first:

1. Stop persisting secret-bearing tool arguments.
2. Remove URL and `localStorage` bearer-token handling from the dashboard.
3. Scrub browser auth session state after completion.

Those three changes would eliminate the most credible credential leak paths in the current design.
