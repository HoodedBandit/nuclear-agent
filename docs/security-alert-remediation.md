# Security Alert Remediation Inventory

Baseline: `main` at `f1bfcbe` after the green GA/CodeQL run on 2026-04-21.

Local verification after remediation:

- `cargo fmt --all --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `python -m unittest discover scripts/tests`
- `scripts/verify-workspace.ps1`
- `scripts/verify-ga.ps1`

GitHub CodeQL status is pending because these changes have not been pushed yet.

| Alert | Rule | Sink | Disposition | Regression coverage | Final status |
| --- | --- | --- | --- | --- | --- |
| 80 | `py/clear-text-storage-sensitive-data` | `scripts/harness/common.py:69` | Split raw config writer from sanitized artifact writer; keep raw use limited to local harness config fixtures. | Python harness sanitization tests. | Local gates passed; pending GitHub CodeQL |
| 79 | `rust/path-injection` | `crates/agent-core/src/safety.rs:182` | Harden path helper boundaries and keep canonicalization rooted in validated ancestors. | Agent core path safety tests. | Local gates passed; pending GitHub CodeQL |
| 67 | `rust/path-injection` | `crates/agent-daemon/src/memory.rs:807` | Resolve `AGENTS.md` only from canonical workspace roots. | Daemon memory profile path tests. | Local gates passed; pending GitHub CodeQL |
| 66 | `rust/path-injection` | `crates/agent-daemon/src/memory/guidance.rs:24` | Validate skill names and skill-file lookup under the managed skills root. | Daemon skill guidance path tests. | Local gates passed; pending GitHub CodeQL |
| 94 | `rust/cleartext-transmission` | `crates/agent-providers/src/oauth.rs:49` | Use sink-local validated OAuth token endpoint before authorization-code exchange. | OAuth endpoint validation tests. | Local gates passed; pending GitHub CodeQL |
| 95 | `rust/cleartext-transmission` | `crates/agent-providers/src/oauth.rs:177` | Use sink-local validated OAuth token endpoint before refresh. | OAuth endpoint validation tests. | Local gates passed; pending GitHub CodeQL |
| 96 | `rust/cleartext-transmission` | `crates/agent-providers/src/oauth.rs:301` | Use sink-local validated OAuth token endpoint before OpenAI refresh. | OAuth endpoint validation tests. | Local gates passed; pending GitHub CodeQL |
| 81 | `rust/cleartext-logging` | `crates/agent-cli/src/config_cli.rs:336` | Route model display through a display-safe helper that preserves non-secret model labels while redacting token-like content. | CLI display-safety tests and phase2 model-list smoke. | Local gates passed; pending GitHub CodeQL |
| 82 | `rust/cleartext-logging` | `crates/agent-cli/src/cli_support.rs:383` | Route session display through fingerprinted display helper. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 5 | `rust/cleartext-logging` | `crates/agent-cli/src/connector_cli.rs:785` | Route Signal connector identifiers/account fields through display-safe helpers. | CLI display-safety tests and phase2 connector matrix smoke. | Local gates passed; pending GitHub CodeQL |
| 83 | `rust/cleartext-logging` | `crates/agent-cli/src/connector_cli.rs:828` | Route Signal connector detail fields through display-safe helpers. | CLI display-safety tests and phase2 connector matrix smoke. | Local gates passed; pending GitHub CodeQL |
| 7 | `rust/cleartext-logging` | `crates/agent-cli/src/connector_cli.rs:901` | Route Signal configure output through display-safe helpers. | CLI display-safety tests and phase2 connector matrix smoke. | Local gates passed; pending GitHub CodeQL |
| 89 | `rust/cleartext-logging` | `crates/agent-cli/src/main.rs:1718` | Route run response metadata through display-safe helpers. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 16 | `rust/cleartext-logging` | `crates/agent-cli/src/main.rs:2003` | Route Signal slash-command output through display-safe helpers. | CLI display-safety tests and phase2 connector matrix smoke. | Local gates passed; pending GitHub CodeQL |
| 90 | `rust/cleartext-logging` | `crates/agent-cli/src/main.rs:2612` | Fingerprint compacted session IDs. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 91 | `rust/cleartext-logging` | `crates/agent-cli/src/main.rs:2806` | Fingerprint forked session IDs. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 92 | `rust/cleartext-logging` | `crates/agent-cli/src/main.rs:2894` | Route run response metadata through display-safe helpers. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 93 | `rust/cleartext-logging` | `crates/agent-cli/src/main.rs:2932` | Fingerprint forked session IDs. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 85 | `rust/cleartext-logging` | `crates/agent-cli/src/operations_cli.rs:483` | Fingerprint mission checkpoint session IDs. | CLI display-safety tests. | Local gates passed; pending GitHub CodeQL |
| 84 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:309` | Route discovered hosted model output through display-safe helper that preserves non-secret labels and redacts token-like content. | CLI display-safety tests and phase2 model-list smoke. | Local gates passed; pending GitHub CodeQL |
| 10 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:364` | Route detected local model output through display-safe helper that preserves non-secret labels and redacts token-like content. | CLI display-safety tests and phase2 model-list smoke. | Local gates passed; pending GitHub CodeQL |
| 11 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:375` | Route detected local model output through display-safe helper that preserves non-secret labels and redacts token-like content. | CLI display-safety tests and phase2 model-list smoke. | Local gates passed; pending GitHub CodeQL |
| 86 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:533` | Do not print OAuth URLs; on browser-launch failure, write a local fallback HTML file and print only its path plus a URL fingerprint. | CLI auth fallback-file display tests. | Local gates passed; pending GitHub CodeQL |
| 87 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:838` | Do not print OAuth URLs; on browser-launch failure, write a local fallback HTML file and print only its path plus a URL fingerprint. | CLI auth fallback-file display tests. | Local gates passed; pending GitHub CodeQL |
| 88 | `rust/cleartext-logging` | `crates/agent-cli/src/provider_auth.rs:1527` | Do not print OAuth URLs; on browser-launch failure, write a local fallback HTML file and print only its path plus a URL fingerprint. | CLI auth fallback-file display tests. | Local gates passed; pending GitHub CodeQL |
| 21 | `rust/cleartext-logging` | `crates/agent-providers/src/anthropic.rs:38` | Ensure provider error rendering uses a redacted error wrapper. | Provider redaction tests. | Local gates passed; pending GitHub CodeQL |
| 22 | `rust/cleartext-logging` | `crates/agent-providers/src/anthropic.rs:110` | Ensure provider error rendering uses a redacted error wrapper. | Provider redaction tests. | Local gates passed; pending GitHub CodeQL |
| 23 | `rust/cleartext-logging` | `crates/agent-providers/src/openai_compatible.rs:25` | Ensure provider error rendering uses a redacted error wrapper. | Provider redaction tests. | Local gates passed; pending GitHub CodeQL |
| 24 | `rust/cleartext-logging` | `crates/agent-providers/src/openai_compatible.rs:64` | Ensure provider error rendering uses a redacted error wrapper. | Provider redaction tests. | Local gates passed; pending GitHub CodeQL |
| 25 | `rust/cleartext-logging` | `crates/agent-providers/src/openai_compatible.rs:125` | Ensure provider error rendering uses a redacted error wrapper. | Provider redaction tests. | Local gates passed; pending GitHub CodeQL |
