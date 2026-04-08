# Dependency Policy

This workspace treats dependency drift as an engineering quality issue, not a cleanup chore.

The current policy has three rules:

1. Shared direct dependencies belong in `[workspace.dependencies]`.
2. New duplicate crate families are blocked unless they are explicitly justified.
3. `cargo tree --workspace --duplicates` is diagnostic output; `cargo deny check bans` is the enforcement gate.

## Must-Unify Families

These duplicate families were caused by direct dependency choices we control and have been removed:

- `windows-sys 0.60.x` from the old `arboard` and `keyring` line
- `windows-sys 0.45.x` from the old `webbrowser` -> `jni` line
- `thiserror 1.x`
- `core-foundation 0.9.x`

The current workspace uses:

- `copypasta` instead of `arboard`
- `opener` instead of `webbrowser`
- `keyring-core` plus platform-specific stores instead of `keyring`

That eliminated the meaningful Windows drift we directly owned.

## Accepted Duplicate Families

The remaining duplicates are currently accepted because they are upstream semver splits or target-specific support stacks rather than direct workspace drift.

### Runtime and build ecosystem splits

- `foldhash 0.1.5`
  - Anchored by `rusqlite` -> `hashlink`
  - Newer `ratatui-core` is already on `foldhash 0.2`
- `hashbrown 0.15.5`
  - Anchored by `rusqlite` -> `hashlink`
  - TUI stack is on `hashbrown 0.16`
- `getrandom 0.2.17`
  - Anchored by `ring` and part of the TLS stack
- `getrandom 0.4.2`
  - Anchored by `tempfile` via `dialoguer`
- `r-efi 5.3.0`
  - Comes from the `getrandom 0.3` lineage while `getrandom 0.4` uses `r-efi 6`
- `rand 0.8.5`
  - Anchored by `scraper` / `phf`
- `rand_core 0.6.4`
  - Part of the `rand 0.8` lineage

### Platform-target splits

- `objc2 0.5.2`
  - Anchored by `copypasta` on macOS
- `objc2-foundation 0.2.2`
  - Anchored by `copypasta` on macOS
- `windows-sys 0.52.0`
  - Anchored by `ring`
- `windows-sys 0.59.0`
  - Anchored by `dbus-secret-service-keyring-store`

These are intentionally allowlisted in `deny.toml`. Anything outside this list is treated as new drift and should fail CI.

## Drift Checks

The workspace uses three drift checks:

1. `cargo deny check bans`
   - Blocks unapproved duplicate families.
2. `python scripts/check-workspace-dependency-drift.py`
   - Blocks shared direct dependencies that bypass `[workspace.dependencies]`.
3. `cargo outdated -R`
   - Report-only visibility for upgrade drift.

## Update Procedure

When a new duplicate appears:

1. Run `cargo tree --workspace --duplicates`.
2. Identify whether the duplicate is directly caused by this workspace or is an upstream split.
3. If we control it, fix the dependency graph.
4. If we do not control it and the split is justified, document it here and add the minimal allowlist entry in `deny.toml`.

The allowlist should stay small and commented. If it starts growing, the graph needs cleanup instead of more exceptions.
