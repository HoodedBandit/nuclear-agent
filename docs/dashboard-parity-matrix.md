# Browser Dashboard Parity Matrix

This matrix is the historical parity record for the browser cockpit migration. The modern dashboard is now the only shipped browser UI, and the legacy browser surface was removed only after every workflow proven in the classic Playwright suite was covered by the modern dashboard and its modern Playwright suite.

## Covered legacy workflows

| Legacy workflow | Modern browser coverage |
| --- | --- |
| Dashboard auth/connect/bootstrap | `modern dashboard loads the product shell and keeps the main layout panes separated` |
| Alias switch and promote-to-main | `modern chat promotes the selected alias to main and keeps it after reload` |
| Chat task execution and transcript streaming | `modern chat runs a task, preserves task mode, and exposes the resume packet` |
| Control-socket reconnect/fallback guard | `modern chat keeps control-socket semantics and does not fall back to HTTP streaming` |
| Task mode restore on saved session reopen | `modern chat restores task mode when a saved session is reopened` |
| Session resume packet visibility | `modern chat runs a task, preserves task mode, and exposes the resume packet` and `modern operations workbench rebuilds and searches memory` |
| Guided provider creation | `modern providers workbench creates a provider and alias` |
| Launchpad shortcuts into setup workbenches | `modern overview quick launch opens the correct guided workbenches` |
| Connector setup and management | `modern connectors workbench creates an inbox connector` |
| Workspace inspection | `modern dashboard loads the product shell and keeps the main layout panes separated` |
| Plugin lifecycle management | `modern plugins workbench installs, updates, and removes a plugin` |
| Advanced config load/edit/save | `modern system workbench loads and saves config and preserves self-edit when enabling autonomy` |
| Top-level product workbench switching | `modern navigation swaps product workbenches cleanly without leaving stale panels active` |
| Memory rebuild/search/review and resume evidence | `modern operations workbench rebuilds and searches memory` |
| Slash commands and shell commands in chat | `modern chat runs slash commands and shell commands from the cockpit` |
| Autonomy self-edit preservation | `modern system workbench loads and saves config and preserves self-edit when enabling autonomy` |
| Remote-content safety surfacing | `modern chat surfaces remote-content safety events` |

## Cutover outcome

The legacy browser surface was deleted only after:

1. The modern Playwright suite covers every workflow above.
2. The deterministic workspace gate is green.
3. No public daemon route, embedded asset, or operator hint still points to the classic dashboard.
