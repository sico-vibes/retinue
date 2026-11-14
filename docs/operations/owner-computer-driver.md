# The owner's computer driver

The Windows computer-use driver Phase C runs behind Retinue's `owner_*` tools
(`crates/retinue-daemon/src/owner_computer.rs`, from
`docs/research/owner-computer-driver-2026-11-14.md`, section 3). The owner's own
computer is the one capability that must keep asking, so the binary that touches it
is pinned, checked on every call, and never committed.

## The pin

| | |
|---|---|
| Upstream | `iFurySt/open-computer-use` (MIT). The mirror `anomalyco/computer-use` publishes no releases, so it cannot be a source of a hash. |
| Release | `v1.1.4`, published 2026-11-07T06:57:33Z |
| Asset | `open-computer-use-cli-1.1.4-windows-amd64.zip` |
| Zip | 1,062,166 bytes, SHA-256 `6635efc579ee3ac1f8b26cfb67e31ccb0ee88f1aa5c587359d17daa780e6817a` |
| Extracted | `open-computer-use.exe`, 2,467,328 bytes, SHA-256 `c8ca36d8871f02e621829f6024a794f186c779217a02b9b5cc60161917e90854` |
| Licence | MIT, "Copyright (c) 2026 Leo" (`LICENSE` in the repository) |
| Signature | **Not Authenticode-signed** (`Get-AuthenticodeSignature` on the extracted exe: `NotSigned`), so the pin is version plus hash, not a publisher. |
| Pinned on | 14 November 2026 |
| Taken by | the backend lane (Gogo), by fetching the release asset and hashing both the zip and the extracted exe |

The same two figures are constants in the daemon (`DRIVER_VERSION`,
`DRIVER_ZIP_SHA256`, `DRIVER_EXE_SHA256`) and in
`scripts/fetch-owner-computer-driver.mjs`. The daemon checks the **exe's** hash
before every start and refuses a different binary rather than warning about it.

## How it is fetched

```
node scripts/fetch-owner-computer-driver.mjs             # beside `retinued.exe` (target/debug)
node scripts/fetch-owner-computer-driver.mjs --release    # target/release
node scripts/fetch-owner-computer-driver.mjs --dest <dir>
```

The script downloads the release asset from GitHub, checks the zip's SHA-256,
expands it in a temporary folder, checks the exe's SHA-256, copies it to the
daemon's folder and removes the temporary files. A wrong hash stops it with
nothing staged. Nothing is committed: `.gitignore` keeps `open-computer-use.exe`
out of the tree, and the installer will need it added to `externalBin` in
`apps/desktop/src-tauri/tauri.conf.json` before the feature ships to users.

The daemon looks for the driver in two places, in this order:

1. next to `retinued.exe` (where the installer puts its sidecars);
2. `<data>/owner-computer/open-computer-use.exe` (a development build or a
   per-machine copy).

## What it is and is not

- It speaks MCP over **stdio, newline-delimited JSON** (not `Content-Length`
  framing): `initialize`, then `tools/call`. It answers `protocolVersion`
  `2025-03-26`.
- Nine tools, all app-scoped: `list_apps`, `get_app_state`, `click`,
  `perform_secondary_action`, `scroll`, `drag`, `type_text`, `press_key`,
  `set_value`. No shell, no file access, no registry, no process control.
- Actions use UI Automation patterns first and window messages second, so the
  owner's foreground focus and pointer are not taken. Real pointer and real
  keyboard input are gated behind
  `OPEN_COMPUTER_USE_ALLOW_GLOBAL_POINTER_FALLBACKS`, which Retinue **never sets
  and strips from the child's environment** (`driver_env` in the daemon).
- `get_app_state` returns a **screenshot by default** (`include_screenshot: true`).
  Retinue writes `include_screenshot: false` explicitly on every observation, and
  the first cut has no `owner_screenshot` tool at all.
- It runs `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File
  <temp script>` per call, so expect roughly a second of overhead before an app
  reacts.

## What was checked in this pin, and what was not

Checked by reading the v1.1.4 `apps/OpenComputerUseWindows/` source and by running
the binary on the owner's machine:

- The nine tools and their schemas match the release's own `tools/list` payload.
- `get_app_state` honours `include_screenshot: false` (its answer says
  "Screenshot omitted by include_screenshot=false").
- The tree format is one line per node: `<tab depth><index> <role> <name>
  [Value: …] [Secondary Actions: …] [Frame: …]`, with a `Window:` header line.
- **v1.1.4 does not report UI Automation's `IsPassword`.** The research report's
  fail-closed masking claim (`Test-PasswordElement`) is **not** in this release: its
  `Get-ElementValue` returns whatever the `ValuePattern` gives it. Retinue
  therefore masks the tree itself (`mask_tree`, `nameless_edit`): a value is
  blanked when the element's own name, class or role names a secret, when the value
  is only mask characters, or when the control is an edit the app did not name.
  This is a heuristic, not the flag, which is why the deny-list and the attended
  conversation are the controls that actually hold.

Not verified here: a real Chrome/Electron page tree, a password field in a real
sign-in form, DPI behaviour at 150%/200%, and a click landing on a second monitor.
Those are the five smoke checks in section 3.7 of the research report and belong
to the owner's live check before a release that turns the feature on.

## How to upgrade

1. Read the release diff for `apps/OpenComputerUseWindows/` — it changes weekly.
2. Fetch the new asset and read its figures **at fetch time**: the zip's SHA-256,
   the exe's SHA-256, and whether it is signed.
3. Run the smoke list from research section 3.7 on the owner's machine.
4. Update `DRIVER_VERSION`, `DRIVER_ZIP_SHA256` and `DRIVER_EXE_SHA256` in
   `crates/retinue-daemon/src/owner_computer.rs` and in
   `scripts/fetch-owner-computer-driver.mjs`, and this table.
5. Run `cargo test -p retinue-daemon --bin retinued phase2a_tests` (the
   `owner_*` tests) and the daemon's remote policy tests.
