# Nuclear Agent

Nuclear Agent is a Rust-based local agent runtime for Windows and Linux.

It ships as:

- a persistent local daemon with an authenticated HTTP and WebSocket control plane
- a terminal-first `nuclear` CLI with interactive, non-interactive, and TUI flows
- a browser dashboard for live status, providers, sessions, plugins, and operator controls
- local persistence for config, sessions, logs, missions, memory, and plugin state

## What It Supports

- hosted providers: OpenAI-compatible, Anthropic, Moonshot, OpenRouter, and Venice
- local providers: Ollama and self-hosted OpenAI-compatible endpoints
- model aliases and a configurable `main` target
- tool calling with trust and permission gates
- multimodal image attachments where the provider supports them
- plugin, MCP, and app-style tool integration
- memory, missions, delegation, autonomy, autopilot, and evolve flows
- rollback and redacted support-bundle export for managed installs

## Workspace Layout

- `crates/agent-core`: shared contracts and request/response types
- `crates/agent-storage`: persistence, paths, migration, and install metadata
- `crates/agent-providers`: provider adapters and credential storage
- `crates/agent-policy`: trust, permission, and autonomy helpers
- `crates/agent-daemon`: runtime, tools, control plane, and dashboard assets
- `crates/agent-cli`: the `nuclear` command and terminal UX
- `scripts/`: verification, packaging, install, rollback, and release tooling
- `harness/`: deterministic and reference evaluation tasks and fixtures
- `tests/`: dashboard end-to-end coverage

## Install

### Managed Installers

Packaged installs use:

- `install.ps1` on Windows
- `install.cmd` as a Windows wrapper
- `install` on Linux

Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

Linux:

```bash
./install
```

Managed upgrades migrate legacy install roots and state into the canonical Nuclear paths. New installs write only the canonical `nuclear` layout.

### Install From Source

Build the workspace:

```powershell
cargo build --workspace
```

Install the CLI locally from source:

```powershell
cargo install --path crates/agent-cli --force
```

## Quick Start

Run onboarding:

```powershell
target\debug\nuclear.exe setup
```

Or just launch the CLI:

```powershell
target\debug\nuclear.exe
```

If no usable configuration exists, the CLI enters onboarding before opening chat.

Useful daemon commands:

```powershell
target\debug\nuclear.exe daemon start
target\debug\nuclear.exe daemon status
target\debug\nuclear.exe daemon stop
```

Useful interactive commands:

```text
/help
/model <alias>
/thinking high
/permissions full-auto
/attach path\to\image.png
/copy
/compact
/fork
/resume
/new
!pwd
```

Non-interactive runs:

```powershell
target\debug\nuclear.exe exec "Summarize the current workspace"
target\debug\nuclear.exe review --uncommitted
target\debug\nuclear.exe run --task main="Write the backend plan" --task secondary="Write the release notes"
```

## Providers

Add a named hosted provider:

```powershell
target\debug\nuclear.exe provider add --id anthropic --name Anthropic --kind anthropic --model claude-3-7-sonnet --api-key %ANTHROPIC_API_KEY%
```

Log in through the guided flow:

```powershell
target\debug\nuclear.exe login --kind openai-compatible
target\debug\nuclear.exe login --kind anthropic
target\debug\nuclear.exe login --kind moonshot
target\debug\nuclear.exe login --kind openrouter
target\debug\nuclear.exe login --kind venice
```

Add a local provider:

```powershell
target\debug\nuclear.exe provider add-local --id ollama-local --name Ollama --kind ollama --main-alias main
```

List models for a configured provider:

```powershell
target\debug\nuclear.exe model list --provider anthropic
target\debug\nuclear.exe model list --provider ollama-local
```

## Dashboard

Launch the dashboard:

```powershell
target\debug\nuclear.exe dashboard
```

The dashboard exposes:

- provider and alias management
- live status and logs
- session resume and chat
- plugin and connector management
- config editing and operator controls

## Operations

Key operator commands:

```powershell
target\debug\nuclear.exe doctor
target\debug\nuclear.exe plugin doctor
target\debug\nuclear.exe support-bundle
```

For managed installs, rollback companions are installed with the package and can restore the previous managed binary. See [docs/operations.md](docs/operations.md).

## Verification

Fast workspace verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-workspace.ps1
```

GA verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-ga.ps1
```

Release packaging:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\finalize-release.ps1 -Token "<daemon-token>" -Workspace .
```

Linux equivalents are provided in the matching `.sh` scripts. See [docs/harness.md](docs/harness.md) and [docs/release-checklist.md](docs/release-checklist.md).
