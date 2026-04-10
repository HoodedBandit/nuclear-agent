# Development Handoff

`J:\Nuclear AI box\Nuclear Agent final` is now the canonical development workspace.

Legacy folders are reference-only:

- `J:\Nuclear AI box\Agent builder.repair`
- `J:\Nuclear AI box\Nuclear Agent release build`

GitHub `main` currently reflects the repaired line from `Agent builder.repair`, not the older `release build` branch snapshot.

## Current State

- This workspace is a clean clone of GitHub `main`.
- Current `main` includes the later CI, installer, dependency, refactor, and verification fixes.
- Current `main` ships the daemon-served static dashboard.
- The older React/Vite modern dashboard line still exists only in the legacy `release build` workspace and may need selective porting if that browser direction is still desired.

## High-signal roadmap

- Reconcile only the release-build-only assets and behavior that are still wanted.
- Finish dashboard productization from the canonical workspace instead of reviving legacy folders.
- Keep packaging, verification, and release work in `J:\Nuclear AI box\Nuclear Agent final` only.

## Most likely missing set to audit from `release build`

- `ui/dashboard`
- `crates/agent-daemon/static-modern`
- related modern dashboard tests and docs

## Working rule

Treat the two old folders as evidence sources, not active development roots.
