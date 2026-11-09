# FIX11: Retinue installed identity (installer, executables, data directory)

> **What this is:** Spec for FIX11: the installed Retinue identity (installer, executables and data directory).
> **Status:** historical
> **Last updated:** 2026-10-15

Written 15 October 2026. Owner directive: change the installer name "and so on" now
that the product is Retinue (FIX9 renamed the visible text and `productName`). The
risk is the user's data: today everything lives under
`%LOCALAPPDATA%\com.retinue.desktop` (Tauri `app_local_data_dir`, which also holds
the WebView2 profile, the SQLite database and downloaded speech models). **No
existing user may lose conversations, settings or downloaded models.**

Do not edit anything under `apps/desktop/src/` (UI) and do not change user-visible
copy; another lane (FIX12) owns those.

## 1. What changes

| Item | Now | After |
|---|---|---|
| Bundle identifier | `com.retinue.desktop` | `com.retinue.desktop` (dev: `com.retinue.desktop.dev` in `scripts/dev-windows.ps1`) |
| Main executable | cargo package name | `Retinue.exe` (`mainBinaryName` in `tauri.conf.json`) |
| Daemon sidecar | `retinued` | `retinued` (`[[bin]]` name in `crates/retinue-daemon/Cargo.toml`, `externalBin`, every script, `.gitignore` pattern) |
| Hook sidecar | `retinue-hook` | `retinue-hook` (same places) |
| Installer file | `Retinue_<v>_x64-setup.exe` pattern in scripts | `Retinue_<v>_x64-setup.exe` (scripts, `make-update-manifest.mjs`, `release-windows.ps1`, `docs/operations/release-checklist.md` template text) |
| Shortcut / uninstall / install dir | follow `productName` | confirm they read "Retinue" in the generated NSIS script |
| Private temp folder | `%LOCALAPPDATA%\Retinue\private-tmp` | `%LOCALAPPDATA%\Retinue\private-tmp` (`retinue-adapter-claude`, daemon) |
| Speech models folder | `...\Retinue` under local data (`speech.rs` ~line 99) | `...\Retinue`, migrated (see 2) |

Keep, deliberately (state them in the report): crate and package names `retinue-*`,
npm names, the `retinued.ready` protocol message type, JSON-RPC method names,
setting keys, `RETINUE_*` environment variables, `retinue.*` localStorage keys and
`retinue-*` Tauri event names, and the GitHub repository slug. Accept `RETINUE_*`
only where it is trivial; otherwise do nothing.

## 2. Data migration (the important part)

Add `apps/desktop/src-tauri/src/migrate.rs` (pure functions over paths so it is
unit-testable with temp dirs) and call it first in `run()`/`main()` **before** the
Tauri builder creates the WebView2 profile.

```
legacy = %LOCALAPPDATA%\com.retinue.desktop     (and ...\com.retinue.desktop.dev for dev builds)
target = %LOCALAPPDATA%\com.retinue.desktop     (and ...dev)
```

Rules:

1. Nothing to do when `legacy` does not exist, or when `target` already holds the
   database file (the migration already ran or the user started fresh).
2. If `target` does not exist: `fs::rename(legacy, target)` (atomic on one volume).
3. If `target` exists but is only an empty/fresh shell (no database, no
   `migrated-from-retinue.json`): remove the shell, then rename as in 2.
4. If the rename fails (the old app still running, a locked file), retry for up to
   ~5 s, then fall back to a recursive copy that skips files it cannot open. The
   legacy directory is **never deleted** in the fallback.
5. After success write `migrated-from-retinue.json` in `target` with
   `{ "from": "<legacy path>", "at": "<RFC 3339>", "method": "rename"|"copy", "skipped": [..] }`.
6. Never read, parse or log file contents; only paths and counts.
7. Return `MigrationOutcome::{NotNeeded, Moved, Copied { skipped }, Failed(String)}`;
   on `Failed` the app still starts (with an empty profile) and logs one line.
8. Also move `%LOCALAPPDATA%\Retinue` (speech models, private tmp) to
   `%LOCALAPPDATA%\Retinue` by the same rules.
9. Autostart: if `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` has a value
   named `Retinue`, remove it and, if the app's autostart setting is enabled, make
   sure the new entry exists (use the same mechanism the app already uses; do not
   add a registry crate if one is not already a dependency, `reg.exe` is fine).

Tests (temp dirs): move, nothing-to-do, existing fresh shell, copy fallback with a
locked file, marker content, idempotence on a second call.

## 3. Installer behaviour

- Read the generated NSIS template for the Tauri version in use (cargo registry
  `tauri-bundler` `installer.nsi`) and record in the report what uninstall key and
  install directory it derives from `productName`.
- Add an NSIS pre-install hook (`bundle.windows.nsis.installerHooks`) that, when an
  old per-user "Retinue" install is registered, stops a running `Retinue.exe`/`retinued.exe`
  and runs its uninstaller silently **without removing app data**, so the old app
  does not sit next to the new one. Best-effort and logged; a failure must not
  abort the install.
- State plainly in the report that the installer cannot be exercised without a
  build; list what a human must check after a build.

## 4. Gates

`cargo test --workspace --exclude retinue-desktop`, `cargo test -p retinue-desktop`
for the migration tests if the crate builds in this environment (otherwise run the
module's tests by path), `cargo clippy --workspace --exclude retinue-desktop
--all-targets -- -D warnings`, `npm run typecheck`, `npm run test`. Never commit;
leave the diff and write `docs/history/fix-11/report.md`.
