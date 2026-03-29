# Beta Release Notes

## Summary

Nuclear Agent `0.8.0` is the first beta release candidate for the local Rust
agent runtime. Windows is the blocking release platform for this beta. Linux
compatibility is preserved, but Linux is not the manual release signoff target.

## Highlights

- Canonical CLI name is `nuclear`; the legacy `autism` launcher is retained for
  compatibility only.
- Windows packaged installs prefer a bundled release binary and fall back to a
  source build automatically if local application-control policy blocks the
  packaged executable.
- The daemon, dashboard, TUI, session lifecycle, provider flows, plugin review
  lifecycle, connector surfaces, and benchmark harnesses are all included in the
  beta scope.
- Release verification now includes Phase 1 runtime smoke, Phase 2 operator
  certification, packaged installer smoke, clippy, dashboard E2E, prerelease
  benchmarks, soak artifacts, and generated release records.

## Compatibility Notes

- `nuclear` is the supported day-to-day command name in docs and examples.
- `autism` is still installed as a compatibility launcher for existing scripts,
  PATH entries, and legacy install roots.
- Fresh Windows installs default to `%LOCALAPPDATA%\Programs\NuclearAI\Nuclear`.
- Existing legacy Windows installs under
  `%LOCALAPPDATA%\Programs\NuclearAI\Autism` upgrade in place.

## Known Beta Limits

- Anthropic, Moonshot, and Venice still expose a browser helper path that
  captures API-key entry rather than a first-party Codex-style account-login
  flow.
- The soak harness requires a live daemon token and is not meaningful without a
  configured local daemon profile.
- Linux compatibility is preserved, but Windows remains the only blocking beta
  signoff platform.

## Verification Commands

Windows core beta verification:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-beta.ps1
```

Windows final signoff with packaging and release record generation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\verify-phase3.ps1 -Token "<daemon-token>" -Workspace .
```
