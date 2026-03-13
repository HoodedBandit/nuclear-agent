# Recovery Report

Date: 2026-03-12

## Outcome

`Agent builder.repair` is a clean, buildable recovery workspace assembled from the restored `Agent builder` tree.

## Sources Used

- Primary source: recovered working tree under `J:\Nuclear AI box\Agent builder`
- Secondary evidence: duplicate `_0` files from the recovered tree
- Deferred evidence: unpacked `dist\autism-cli-bundle-*` source trees and `_restore_7e`
- Restored reference asset: `codex-main/codex-rs/core/models.json`

## Repair Actions

- Built a new workspace at `J:\Nuclear AI box\Agent builder.repair`
- Copied only canonical live-tree files into the repaired workspace
- Quarantined every `_0` duplicate under `_recovery_quarantine/duplicates`
- Quarantined nested per-crate `.git` directories under `_recovery_quarantine/nested-git`
- Quarantined the damaged root `.git` metadata under `_recovery_quarantine/root-git`
- Excluded `target`, `dist-test`, and `_restore_7e` from the repaired live workspace

## Notes

- All `_0` duplicates in the recovered source tree were byte-identical to their canonical counterparts.
- The root `.git` metadata from the restored tree was incomplete and is not active in the repaired workspace.
- The unpacked bundle `source` trees appear encoded or padded with NUL bytes and were not used as live source replacements.

## Verification

- `cargo check --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Remaining Work

- Optional: attempt Git history reconstruction separately from the repaired working tree
- Optional: compare later bundle snapshots or recovered artifacts only if newer content is suspected to be missing
