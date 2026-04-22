# Security Alert Remediation Inventory

Baseline: `main` at `5f996b5` after the green GA verification run on 2026-04-22.

Current target set before this pass:

- 14 open CodeQL alerts
- 9 `rust/cleartext-logging`
- 3 `rust/cleartext-transmission`
- 1 `rust/path-injection`
- 1 `py/clear-text-storage-sensitive-data`

Local verification after remediation:

- `cargo fmt --all --check`: passed
- `cargo check --workspace`: passed
- `cargo test --workspace`: passed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed
- `python -m unittest discover scripts/tests`: passed
- `scripts/verify-workspace.ps1`: passed
- `scripts/verify-ga.ps1`: passed

GitHub CodeQL status:

- `8021edf` and `24010d1` passed CodeQL, but `ga-verify` exposed a Windows-only path-test assumption that was fixed in `5adcc8b`.
- `5adcc8b` passed CodeQL and `ga-verify` on Ubuntu and Windows.
- Post-`5adcc8b` open alerts still included analyzer-taint findings around authenticated provider sends, OAuth token sends, sanitized harness artifact writes, and provider-discovered model names. The follow-up pass below adds transport hardening and artifact-write hardening while preserving model-list output.
- Post-`2ec2bf9` CodeQL cleared the Rust transport/logging findings; one Python artifact-storage finding remained at the sanitized writer sink, so the final follow-up writes only the double-sanitized UTF-8 byte payload.

| Alert | Rule | Sink | Disposition | Regression coverage | Final status |
| --- | --- | --- | --- | --- | --- |
| 79 | `rust/path-injection` | `crates/agent-core/src/safety.rs:208` | Replace existing-ancestor probing with validated lexical absolute normalization so tainted paths are not used for filesystem existence or canonicalization checks. | Agent core path safety tests. | Local gates passed; pending GitHub CodeQL |
| 94 | `rust/cleartext-transmission` | `crates/agent-providers/src/oauth.rs:49` | Build authorization-code token requests through a sink-local validated token endpoint helper; follow-up inlines endpoint scheme validation at the post construction site. | OAuth invalid-token-endpoint and loopback/HTTPS tests. | Local gates passed; pending GitHub CodeQL |
| 95 | `rust/cleartext-transmission` | `crates/agent-providers/src/oauth.rs:177` | Build refresh-token requests through a sink-local validated token endpoint helper; follow-up inlines endpoint scheme validation at the post construction site. | OAuth invalid-token-endpoint and loopback/HTTPS tests. | Local gates passed; pending GitHub CodeQL |
| 96 | `rust/cleartext-transmission` | `crates/agent-providers/src/oauth.rs:306` | Build OpenAI refresh requests through a sink-local validated token endpoint helper and validate API-key exchange URLs before posting. | OAuth invalid-token-endpoint and loopback/HTTPS tests. | Local gates passed; pending GitHub CodeQL |
| 97 | `py/clear-text-storage-sensitive-data` | `scripts/harness/common.py:104` | Remove the generic JSON writer alias so result/status artifacts must use the sanitized artifact writer and raw writes stay limited to explicit config fixtures; follow-up sanitizes both structured payload and serialized JSON before writing the UTF-8 bytes to disk. | Python harness sanitization tests. | Local gates passed; pending GitHub CodeQL |
| 21 | `rust/cleartext-logging` | `crates/agent-providers/src/anthropic.rs:38` | Stop echoing provider response bodies in Anthropic model-list failures; report status plus a static safe classification. Follow-up rejects authenticated remote HTTP provider endpoints before request dispatch. | Provider error redaction tests, provider endpoint transport tests, and phase2 failure-message smoke. | Local gates passed; pending GitHub CodeQL |
| 22 | `rust/cleartext-logging` | `crates/agent-providers/src/anthropic.rs:110` | Stop echoing provider response bodies in Anthropic prompt failures; report status plus a static safe classification. Follow-up rejects authenticated remote HTTP provider endpoints before request dispatch. | Provider error redaction tests and provider endpoint transport tests. | Local gates passed; pending GitHub CodeQL |
| 23 | `rust/cleartext-logging` | `crates/agent-providers/src/openai_compatible.rs:25` | Stop echoing provider response bodies in embedding failures; report status plus a static safe classification. Follow-up rejects authenticated remote HTTP provider endpoints before request dispatch. | Provider error redaction tests and provider endpoint transport tests. | Local gates passed; pending GitHub CodeQL |
| 24 | `rust/cleartext-logging` | `crates/agent-providers/src/openai_compatible.rs:64` | Stop echoing provider response bodies in model-list failures; report status plus a static safe classification. Follow-up rejects authenticated remote HTTP provider endpoints before request dispatch. | Provider error redaction tests, provider endpoint transport tests, and phase2 failure-message smoke. | Local gates passed; pending GitHub CodeQL |
| 25 | `rust/cleartext-logging` | `crates/agent-providers/src/openai_compatible.rs:125` | Stop echoing provider response bodies in completion failures; report status plus a static safe classification. Follow-up rejects authenticated remote HTTP provider endpoints before request dispatch. | Provider error redaction tests and provider endpoint transport tests. | Local gates passed; pending GitHub CodeQL |
| 98 | `rust/cleartext-logging` | `crates/agent-cli/src/config_cli.rs:336` | Validated false-positive candidate: provider-returned model names are operator identifiers, not credentials. `display_safe_model()` preserves normal model IDs for CLI usability but fingerprints token-like, URL-like, JWT-like, or sensitive-key-bearing values. A temporary fingerprint-only change failed GA because it broke the model-list surface, so it was reverted. | Agent core display-safety tests, existing CLI model output coverage, and phase2 model-list smoke. | Local gates passed; pending GitHub CodeQL/dismissal decision |
| 99 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:310` | Same validated false-positive candidate for hosted model discovery output. | Agent core display-safety tests and existing CLI coverage. | Local gates passed; pending GitHub CodeQL/dismissal decision |
| 100 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:365` | Same validated false-positive candidate for local model discovery output. | Agent core display-safety tests and existing CLI coverage. | Local gates passed; pending GitHub CodeQL/dismissal decision |
| 101 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:376` | Same validated false-positive candidate for local model discovery output. | Agent core display-safety tests and existing CLI coverage. | Local gates passed; pending GitHub CodeQL/dismissal decision |

Additional dependency security fix:

| Item | Source | Disposition | Verification |
| --- | --- | --- | --- |
| `RUSTSEC-2026-0104` | `rustls-webpki 0.103.12` via `rustls` | Updated lockfile to `rustls-webpki 0.103.13`. | `cargo audit`, `scripts/verify-workspace.ps1`, and `scripts/verify-ga.ps1` pass. |
