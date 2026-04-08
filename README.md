# Nuclear Agent

CLI-first local agent runtime built in Rust for Windows and Linux.

Current implementation:
- Persistent daemon architecture with an authenticated local HTTP control plane
- Cross-platform terminal client with a full-screen TUI by default and line-mode fallback
- Local config, sessions, logs, and missions persisted on disk
- Multi-model alias routing for main-agent and subagent tasks
- Hosted API-key and OAuth providers, including OpenAI-compatible, Anthropic, Moonshot, OpenRouter, and Venice AI endpoints
- Ollama and self-hosted OpenAI-compatible endpoints for local model hosting
- Multimodal image attachments for OpenAI-compatible, Anthropic, and Ollama requests
- Permission presets for `suggest`, `auto-edit`, and `full-auto`
- Configurable MCP/app connector commands plus skill enablement from `~/.codex/skills`
- Provider-driven tool calling with structured file, patch, git, env, shell, search, and network tools enforced by trust policy
- Explicit high-risk autonomy mode with confirmation gates
- Auto-start configuration for always-on daemon mode

GUI is intentionally deferred until the CLI path is stable.

## Workspace

- `crates/agent-core`: shared types and API contracts
- `crates/agent-storage`: config paths, SQLite persistence, auto-start wiring
- `crates/agent-providers`: provider adapters and keychain-backed secrets
- `crates/agent-policy`: trust and autonomy helpers
- `crates/agent-daemon`: persistent runtime process
- `crates/agent-cli`: terminal client and the user-facing `nuclear` command

## Build

```powershell
cargo build --workspace
```

For a simple terminal command like `codex`, install the CLI binary:

```powershell
cargo install --path crates/agent-cli --force
```

Packaged installs also include:

- `install.ps1` for Windows
- `install` for Linux
- `install.cmd` as a Windows wrapper around `install.ps1`

Those installers place `nuclear` on the user PATH for day-to-day use.
On Windows, if the bundled `nuclear.exe` is blocked by application control, `install.ps1` automatically falls back to building from the packaged source tree and will install `rustup` if needed. When the packaged source tree includes the dashboard E2E harness, `install.ps1` also installs the required npm dependencies and Playwright Chromium browser bundle automatically. Existing legacy managed installs are migrated into the canonical Nuclear root instead of being left in place.

## Quick Start

Run the setup wizard directly:

```powershell
target\debug\nuclear.exe setup
```

Or just launch the CLI in a terminal with `nuclear`. If no usable config exists yet, the CLI now drops straight into the onboarding flow before opening chat.

Manual daemon control:

```powershell
target\debug\nuclear.exe daemon start
target\debug\nuclear.exe daemon status
target\debug\nuclear.exe daemon config --mode always-on --auto-start true
target\debug\nuclear.exe daemon stop
```

Add a hosted provider:

```powershell
target\debug\nuclear.exe provider add --id anthropic --name Anthropic --kind anthropic --model claude-3-7-sonnet --api-key %ANTHROPIC_API_KEY% --main-alias claude
```

Add a Moonshot-compatible hosted provider:

```powershell
target\debug\nuclear.exe provider add --id moonshot --name Moonshot --kind moonshot --model kimi-k2 --api-key %MOONSHOT_API_KEY%
```

Add an OpenRouter provider:

```powershell
target\debug\nuclear.exe provider add --id openrouter --name OpenRouter --kind openrouter --model openai/gpt-4.1 --api-key %OPENROUTER_API_KEY%
```

Add a Venice AI provider:

```powershell
target\debug\nuclear.exe provider add --id venice --name Venice --kind venice --model venice-uncensored --api-key %VENICE_API_KEY%
```

Configure a named hosted provider with the guided login flow:

```powershell
target\debug\nuclear.exe login --kind openai-compatible
target\debug\nuclear.exe login --kind anthropic
target\debug\nuclear.exe login --kind moonshot
target\debug\nuclear.exe login --kind openrouter
target\debug\nuclear.exe login --kind venice
```

The guided hosted login flow now offers three auth paths for every named provider:
- Browser sign-in / browser capture
- OAuth (advanced custom flow)
- API key

The first-run onboarding flow now keeps the hosted path closer to Codex: choose the provider, sign in first, let the CLI load the available models for that authenticated account, then pick a model from the discovered list. It also asks for the default permission preset plus trust and shell/network defaults for the current workspace so the CLI is ready to use as soon as setup completes.

Browser behavior is provider-specific:
- OpenAI uses a real browser sign-in flow against the OpenAI account system and stores a ChatGPT/Codex session for the first-party backend.
- OpenRouter uses its native browser callback flow and captures the resulting API key automatically.
- Anthropic, Moonshot, and Venice still expose the browser option, but today that path opens the provider site plus a local browser capture page for API-key entry because those providers do not have an equivalent public Codex-style account-login flow wired into this CLI yet.

Configure an advanced compatible provider with browser OAuth:

```powershell
target\debug\nuclear.exe login --id openai-oauth --name "OpenAI OAuth" --kind openai-compatible --auth oauth --model gpt-4.1
```

Add a local provider:

```powershell
target\debug\nuclear.exe provider add-local --id ollama-local --name Ollama --kind ollama --main-alias main
```

List models exposed by a configured provider:

```powershell
target\debug\nuclear.exe model list --provider openrouter
target\debug\nuclear.exe model list --provider ollama-local
```

Start an interactive terminal session like `codex`:

```powershell
target\debug\nuclear.exe
```

Useful interactive commands:

```text
/help
/model claude
/fast
/thinking high
/status
/permissions full-auto
/attach path\to\diagram.png
/attachments
/copy
/compact
/init
/rename auth-session
/review
/diff
!pwd
/fork
/resume
/new
```

Start an interactive session with an initial prompt:

```powershell
target\debug\nuclear.exe "Summarize the project status"
```

Run a prompt non-interactively:

```powershell
target\debug\nuclear.exe exec "Summarize the project status"
target\debug\nuclear.exe exec --json --output-schema schema.json --output-last-message final.txt "Return deployment metadata"
target\debug\nuclear.exe exec --image diagram.png "Explain this architecture diagram"
```

Run concurrent subagent tasks on different aliases:

```powershell
target\debug\nuclear.exe run --task claude="Write the backend plan" --task chatgpt="Write the release notes"
```

Run a review prompt non-interactively:

```powershell
target\debug\nuclear.exe review --uncommitted
```

Resume or fork a previous terminal session:

```powershell
target\debug\nuclear.exe resume --last
target\debug\nuclear.exe fork --last
target\debug\nuclear.exe session rename <session-id> "Better title"
```

Manage permission presets:

```powershell
target\debug\nuclear.exe permissions
target\debug\nuclear.exe permissions full-auto
```

Register command-backed MCP/app tools:

```powershell
target\debug\nuclear.exe mcp add --id local-shell --name "Local Shell MCP" --description "Bridge tool" --command python --arg scripts\bridge.py --tool-name bridge_tool --schema-file schema.json
target\debug\nuclear.exe app add --id docs --name Docs --description "Search docs" --command python --arg scripts\docs.py --tool-name docs_search --schema-file schema.json
target\debug\nuclear.exe mcp list
target\debug\nuclear.exe app list
```

Manage plugins:

```powershell
target\debug\nuclear.exe plugin install .\examples\echo-plugin --trust
target\debug\nuclear.exe plugin install .\examples\echo-plugin --trust --grant-network
target\debug\nuclear.exe plugin install "git+https://example.com/echo-plugin.git" --trust
target\debug\nuclear.exe plugin install "market:echo-toolkit" --trust
target\debug\nuclear.exe plugin update echo-toolkit
target\debug\nuclear.exe plugin list
target\debug\nuclear.exe plugin doctor
target\debug\nuclear.exe plugin enable echo-toolkit
target\debug\nuclear.exe plugin trust echo-toolkit
target\debug\nuclear.exe plugin grant echo-toolkit --network
target\debug\nuclear.exe plugin revoke echo-toolkit --network
```

Plugins are managed packages with an `agent-plugin.json` manifest. Local paths, `git+...` sources, and `market:...` marketplace entries are copied into the daemon data directory, hashed for integrity, and tracked with both a source reference and a resolved source path. Trust review is now tied to the installed package hash, so plugin updates require a fresh review before runtime projection resumes. High-risk plugin capabilities also require explicit grants for `shell`, `network`, and `full_disk`.

Inspect a workspace with coding-focused repo signals:

```powershell
target\debug\nuclear.exe repo inspect .
target\debug\nuclear.exe repo inspect . --json
```

Docs:
- [`docs/plugins.md`](docs/plugins.md)
- [`docs/operations.md`](docs/operations.md)
- [`docs/benchmarks.md`](docs/benchmarks.md)
- [`docs/release-checklist.md`](docs/release-checklist.md)
- [`docs/reliability.md`](docs/reliability.md)
- [`docs/ga-release-notes.md`](docs/ga-release-notes.md)

Inspect and enable local skills:

```powershell
target\debug\nuclear.exe skills list
target\debug\nuclear.exe skills enable imagegen
target\debug\nuclear.exe skills disable imagegen
```

Generate shell completions:

```powershell
target\debug\nuclear.exe completion powershell
```

Inspect health:

```powershell
target\debug\nuclear.exe doctor
```

Adjust trust settings without forcing unrelated flags:

```powershell
target\debug\nuclear.exe trust --allow-shell false
target\debug\nuclear.exe trust --path "J:\Nuclear AI box\Agent builder"
```

Run the canonical coding harness against a built binary:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-harness.ps1 -Lane coding-deterministic
```

Run the HTTP control-plane soak harness against a live daemon:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\run-soak.ps1 -Token "<daemon-token>" -Workspace .
```

Package the canonical Windows release bundle:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1
```

Run the final release packaging and record-generation flow:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```

## Notes

- Secrets are stored in the OS keychain when an API key or OAuth token is configured.
- On Linux, keychain support depends on an available backend such as Secret Service or keyutils.
- `login` only creates or rewrites the main alias automatically for the first configured provider unless you pass `--main-alias`.
- OpenAI, Anthropic, Moonshot, OpenRouter, and Venice AI all have guided CLI login flows with browser, OAuth, and API-key options. OpenRouter uses a real browser PKCE flow that returns a stored API key; the other named providers use the browser helper path unless you choose OAuth or direct API-key entry.
- `provider add-local --kind ollama` now auto-detects installed models when the local server is reachable; pass `--model` to override detection explicitly.
- `doctor` now validates that each configured default model is actually present on the provider, which is especially useful for local Ollama installs.
- Tool calls are executed locally by the daemon and currently include directory listing, file search, Codex-style `apply_patch`, file read/write/append/replace, copy/move/delete, recursive filename search, shell execution, environment inspection, git status/diff/log/show, path stat, directory creation, and HTTP fetch/request helpers.
- Enabled skills are injected into the daemon prompt from `~/.codex/skills/.../SKILL.md`, MCP/app connectors become dynamic tools when they are enabled and the session is in `full-auto`, and trusted managed plugin tools run through the hosted plugin protocol.
- The interactive terminal loop now supports Codex-style slash commands for help, model alias switching, `fast`/thinking level changes, status, permissions, image attachment management, clipboard copy, session compaction, AGENTS bootstrapping, session rename, review, diff, new chat, resume, and fork.
- The default terminal experience is now a TUI with transcript, input, help overlay, and searchable session picker; pass `nuclear chat --no-tui` if you want the original line-mode loop.
- `AGENTS.md` guidance is loaded automatically from `~/.codex/AGENTS.md` plus any `AGENTS.md` files found from the filesystem root down to the active working directory, with deeper files taking precedence.
- Interactive `!` commands run directly in the local shell, and `!cd <path>` updates the active working directory for the session without sending that command to the model.
- Sessions now carry titles and working directories so `session list`, `resume`, `fork`, and the picker can filter current-project history more effectively.
- `--thinking` is available on the main non-interactive and session commands, and provider adapters now translate thinking levels into provider-native request fields for OpenAI-compatible endpoints, OpenRouter, and Anthropic.
- Headless `exec` now supports `--json`, `--output-schema`, `--output-last-message`, `--ephemeral`, and image attachments.
- `Think For Yourself` mode is intentionally dangerous and unlimited once enabled.
- The daemon auto-start path launches the same `nuclear` binary in hidden daemon mode rather than relying on a second installed executable.
