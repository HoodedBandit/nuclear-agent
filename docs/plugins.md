# Plugins

Plugins are managed packages installed into the daemon-owned data directory. Each plugin package must include an `agent-plugin.json` manifest at its root.

## Current Scope

- Install source: local path, `git+...` repository source, or `market:...` marketplace entry
- Lifecycle: install, update, list, inspect, enable, disable, trust, untrust, pin, unpin, remove, doctor
- Runtime integration: enabled and trusted tools, connector pollers, and provider adapters run as hosted subprocesses with versioned stdin/stdout contracts
- Managed metadata: installs record `source_kind`, `source_reference`, `source_path`, and `integrity_sha256`
- Marketplace index discovery: default `config/plugin-marketplace.json`, or override with `AGENT_PLUGIN_MARKETPLACE_INDEX`

## Manifest

Example `agent-plugin.json`:

```json
{
  "schema_version": 1,
  "id": "echo-toolkit",
  "name": "Echo Toolkit",
  "version": "0.8.0",
  "description": "Sample plugin",
  "compatibility": {
    "min_host_version": 1,
    "max_host_version": 1
  },
  "permissions": {
    "shell": false,
    "network": false,
    "full_disk": false
  },
  "tools": [
    {
      "name": "echo_tool",
      "description": "Echo structured input",
      "command": "python",
      "args": ["tool.py"],
      "input_schema_json": "{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\"}},\"required\":[\"text\"],\"additionalProperties\":false}",
      "cwd": ".",
      "permissions": {
        "shell": false,
        "network": false,
        "full_disk": false
      },
      "timeout_seconds": 30
    }
  ],
  "connectors": [
    {
      "id": "echo-connector",
      "kind": "webhook",
      "description": "Queue a mission when the plugin sees work",
      "command": "plugin-host",
      "args": ["connector.js"],
      "cwd": ".",
      "timeout_seconds": 30
    }
  ],
  "provider_adapters": [
    {
      "id": "echo-provider",
      "provider_kind": "open_ai_compatible",
      "description": "Expose plugin-managed models",
      "command": "plugin-host",
      "args": ["provider.js"],
      "cwd": ".",
      "default_model": "echo-model",
      "timeout_seconds": 30
    }
  ]
}
```

Required manifest behavior:

- `schema_version` must currently be `1`
- `id` may only use letters, numbers, `.`, `_`, and `-`
- At least one capability must be declared
- Tool schemas must contain valid JSON
- `compatibility.min_host_version` may not exceed `compatibility.max_host_version`
- `timeout_seconds`, when set, must be between `1` and `600`

## CLI

Install and inspect a plugin:

```powershell
target\debug\nuclear.exe plugin install .\plugins\echo-toolkit --trust
target\debug\nuclear.exe plugin install "git+https://example.com/echo-plugin.git" --trust
target\debug\nuclear.exe plugin install "market:echo-toolkit" --trust
target\debug\nuclear.exe plugin update echo-toolkit
target\debug\nuclear.exe plugin update echo-toolkit --source "git+https://example.com/echo-plugin.git#main"
target\debug\nuclear.exe plugin list
target\debug\nuclear.exe plugin get echo-toolkit
target\debug\nuclear.exe plugin doctor
```

Toggle trust and runtime state:

```powershell
target\debug\nuclear.exe plugin enable echo-toolkit
target\debug\nuclear.exe plugin trust echo-toolkit
target\debug\nuclear.exe plugin pin echo-toolkit
target\debug\nuclear.exe plugin disable echo-toolkit
target\debug\nuclear.exe plugin untrust echo-toolkit
target\debug\nuclear.exe plugin remove echo-toolkit
```

## Dashboard

The web dashboard now exposes a Plugins panel with:

- source-string install
- enable/disable
- trust/untrust
- pin/unpin
- package update from the recorded source reference
- per-plugin doctor refresh
- remove
- visible source kind, source reference, and integrity hash summary

## Doctor Behavior

Doctor reports currently verify:

- install directory exists
- managed manifest exists and parses
- schema version is supported
- host compatibility range includes the current host version
- tool schemas are valid JSON
- relative tool, connector, and provider-adapter paths resolve inside the installed package
- the current installed package still matches the recorded integrity hash
- enabled plugins are trusted before runtime projection
- tool-name conflicts with existing MCP/app tools or other plugins

## Hosted Tool Protocol

Trusted enabled plugin tools are executed as subprocesses. The daemon sends one JSON request on stdin and expects either:

- a JSON response shaped like `{ "ok": true, "content": "..." }`
- plain text on stdout/stderr as a fallback

Request shape written to stdin:

```json
{
  "host_version": 1,
  "plugin_id": "echo-toolkit",
  "plugin_name": "Echo Toolkit",
  "plugin_version": "0.8.0",
  "tool_name": "echo_tool",
  "workspace_cwd": "C:\\work\\repo",
  "arguments": {
    "text": "hello"
  },
  "shell_allowed": false,
  "network_allowed": false,
  "full_disk_allowed": false
}
```

Environment variables exposed to tool subprocesses:

- `AGENT_PLUGIN_ID`
- `AGENT_PLUGIN_NAME`
- `AGENT_PLUGIN_VERSION`
- `AGENT_PLUGIN_TOOL_NAME`
- `AGENT_PLUGIN_HOST_VERSION`

## Provider Adapter Protocol

Provider adapters receive one JSON request on stdin and respond with JSON tagged by `action`.

- `list_models`: return `{ "action": "list_models", "ok": true, "models": ["..."] }`
- `run_prompt`: return `{ "action": "run_prompt", "ok": true, "reply": { ...ProviderReply... } }`

Environment variables exposed to provider adapters:

- `AGENT_PLUGIN_ID`
- `AGENT_PLUGIN_NAME`
- `AGENT_PLUGIN_VERSION`
- `AGENT_PLUGIN_PROVIDER_ID`
- `AGENT_PLUGIN_PROVIDER_ADAPTER_ID`
- `AGENT_PLUGIN_PROVIDER_DESCRIPTION`
- `AGENT_PLUGIN_PROVIDER_KIND`
- `AGENT_PLUGIN_HOST_VERSION`

## Connector Poll Protocol

Connector pollers receive one JSON request on stdin and respond with:

```json
{
  "ok": true,
  "detail": "",
  "missions": [
    {
      "title": "Plugin Connector Mission",
      "prompt": "Handle the connector event",
      "alias": "main",
      "requested_model": "gpt-5",
      "cwd": "."
    }
  ]
}
```

Environment variables exposed to connector pollers:

- `AGENT_PLUGIN_ID`
- `AGENT_PLUGIN_NAME`
- `AGENT_PLUGIN_VERSION`
- `AGENT_PLUGIN_CONNECTOR_ID`
- `AGENT_PLUGIN_CONNECTOR_DESCRIPTION`
- `AGENT_PLUGIN_HOST_VERSION`

Runtime enforcement notes:

- enabled plugins must also be trusted before runtime projection occurs
- `timeout_seconds` defaults to the daemon shell timeout when omitted
- plain text fallback exists for compatibility, but JSON response mode is the preferred contract

## Review And Permission Grants

- Plugin trust review is tied to the installed package hash via `reviewed_integrity_sha256`.
- `plugin update` invalidates review when package bytes change, so runtime projection stays blocked until the plugin is reviewed again.
- Declared `shell`, `network`, and `full_disk` capabilities require explicit grants in addition to trust review.
- CLI examples:

```powershell
target\debug\nuclear.exe plugin grant echo-toolkit --network
target\debug\nuclear.exe plugin revoke echo-toolkit --network
target\debug\nuclear.exe plugin install .\plugins\echo-toolkit --trust --grant-network
```
