# Lane report: a headless Linux `retinued`, and console pairing (piece 8 of `03-design-and-order.md`)

Lane: Ada (backend). Worktree `backend-task-from-the-director-claude-16`, branch `retinue/backend-task-from-the-director-claude-16`, cut from `b7c3a32`. Date 17 November 2026.
Design: `docs/research/multi-device/04-linux-headless.md` (the whole piece), `03-design-and-order.md` piece 8 and piece 7's "Console approval for a headless host", `docs/research/many-machines-2026-11.md:233-278`. Owner's rule: `AGENTS.md`, "Amendment (17 November 2026): Retinue on many machines", the "Headless Linux daemon for the VPS" bullet.

**This file is written before the code, and updated after each finished piece.** Where a shape changes, the change is stated in the running log at the end, and the shape above it is corrected in place.

---

# 1. What the Director can build from this file

| Question | Answer |
|---|---|
| Does the desktop UI change for this piece? | **No new screen.** One value changes meaning: the pairing request's `platform` is now the host's real OS (`linux`, `windows`, `macos`) instead of always `windows`. Anything that shows it (the pairing dialog, the machine list) will start saying `linux` for the VPS. |
| Are there new RPCs for the UI? | **None.** The console surface below is for the host's own terminal and is `never` for any paired device. |
| New events? | **None.** Console approval goes through `remote.pair.confirm`, so `remote.pairing.completed` and `remote.device.changed` are emitted exactly as the desktop's approval does. |
| New bot tools? | **None.** |
| New settings? | **None.** The daemon reads two new environment variables (`RETINUE_HEADLESS`, and the XDG defaults) and the systemd unit sets them. |

## 1.1 The one reply field that changes meaning

`remote.pairing.requested` (already shipped, unchanged shape) carries `platform`. Sender side it now reads:

```json
{
  "pairingId": "3f2a…",          // string, uuid
  "name": "LAPTOP-7",            // string, the device's own name, max 80
  "kind": "machine",             // "machine" | "phone" | "tablet" | "browser"
  "platform": "linux",           // string, max 80: std::env::consts::OS of the device that asked
  "login": null,                 // string | null, the tailnet login when Tailscale supplied one
  "method": "tailscale",         // "tailscale" (a code) | "native" (a password sign-in)
  "expiresAt": "2026-11-17T21:04:11+00:00"
}
```

Types: every field is a JSON string except `login`, which may be `null`. Nothing else in the shape moves. The desktop's approve card, and piece 7's `remote.pair.pending` read (another lane), both get the honest value from this change alone.

## 1.2 What the VPS looks like to the window once it is up

No new call is needed: the VPS is a machine link like any other (`machine.info`, `machine.sessions`, `machine.conversation`, `machine.start`…), and a phone can open it in a browser through the gateway. Two facts the guide tells the owner, and the UI already shows:

- `machine.info` reports the host's own version string, so the VPS pill says which build it runs.
- `remote.status` on the VPS reports `method: "tailscale"` with a real `url` once `tailscale serve` has published the gateway.

---

# 2. The console surface (host terminal only, never a paired device)

A headless host has no window, so the two window-only pairing methods get a console path. It is the same decision, made by the same code:

| Step | Window today | Console (this lane) |
|---|---|---|
| Make a code (Tailscale) | `remote.pair.begin` from Settings › Remote | `retinued pair` |
| See a waiting request | the approve card, from the `remote.pairing.requested` event | `retinued pair list` |
| Approve it with a tier | `remote.pair.confirm {pairingId, approve: true, tier}` | `retinued pair confirm --tier chat` (or `viewer` / `operator`) |
| Refuse it | `remote.pair.confirm {approve: false}` | `retinued pair deny` |
| Throw the code away | `remote.pair.cancel` | `retinued pair cancel` |

The console commands are a **thin client over the daemon's existing loopback API**, with their own narrow token:

- When the daemon runs headless (no `RETINUE_PARENT_PID`, i.e. not started by the desktop shell) it writes `<data>/console.json`, mode `0600`, in the data folder (`0700`): `{"address":"127.0.0.1:<port>","token":"<32 bytes, base64url>","pid":<n>}`. The file is deleted on a clean exit and rewritten on the next boot; a stale copy names a token the new daemon never accepts.
- That token is **not** the daemon capability. `main_rpc` accepts it only on the `console.` prefix, and only for the six methods below. Every other method still needs the capability, which stays in memory and reaches only the desktop shell, as today.
- The code itself is printed to the terminal and never written to the log, the database, an event or `console.json`.

### 2.1 The `console.*` methods (loopback, console token, class `never`)

`POST /v1/rpc`, `Authorization: Bearer <console token>`, body `{"v":1,"id":"…","method":"console.…","params":{…}}`.

| Method | Params | Reply |
|---|---|---|
| `console.status` | `{}` | `{"version": "0.1.0", "platform": "linux", "headless": true, "dataFolder": "/home/u/.local/share/retinue", "database": "/home/u/.local/share/retinue/retinue.db", "secrets": {"backend": "files", "folder": "/home/u/.local/share/retinue/secrets"}, "remote": {<the `remote.status` reply, unchanged: enabled, method, url, port, servePort, tailscale, hostName, deviceCount, connectedCount, error, native{fingerprint, sans, port, …}>}`. Strings, except `headless` (bool), the numbers in `remote`, and any field `remote.status` already sends as `null`. `secrets.backend` is `"files"` on Linux and `"windows-credential-manager"` on Windows. **This is the whole `remote.status` object**, not a subset (see the log). |
| `console.pair.begin` | `{}` | `{"code": "K7QM-4XPD", "url": "https://vps.tailnet.ts.net/", "expiresAt": "2026-11-17T21:09:02+00:00", "attemptsLeft": 5}` — refused with `conflict` when Remote is off or the method is Native ("Native Remote signs in with a password."). |
| `console.pair.pending` | `{}` | `{"pending": null}` or `{"pending": {"pairingId": "…", "name": "LAPTOP-7", "kind": "machine", "platform": "windows", "login": null, "method": "tailscale", "expiresAt": "…"}}` — the same fields as `remote.pairing.requested` minus nothing. |
| `console.pair.confirm` | `{"pairingId": "…"?, "approve": true, "tier": "chat"}` | `{"approved": true, "device": {"id": "…", "name": "LAPTOP-7", "kind": "machine", "platform": "windows", "tier": "chat", "login": null, "pairedAt": "…", "online": false, "lastSeenAt": null, "revokedAt": null}}`, or `{"approved": false}`. `pairingId` may be omitted: the one waiting request is used. Errors are the window's own, unchanged: `invalid_argument` when `approve` is true and no tier is given, `conflict` when there is no request, it expired, or it was already decided. |
| `console.pair.deny` | `{"pairingId": "…"?}` | `{"approved": false}` (same path as `confirm` with `approve: false`). |
| `console.pair.cancel` | `{}` | `{"cancelled": true}` or `{"cancelled": false}` when no code was open. |

Every `console.*` method is refused with `unauthorized` when the bearer is not the console token, and with `unsupported` before the capability check when the daemon is not headless (no token file exists, so no client can present one).

### 2.2 `retinued` command line

```
retinued                         run the daemon (as today; headless when no desktop shell started it)
retinued --version               print "retinued <version> (protocol <n>)" and exit
retinued pair                    print the URL and the code, then wait for a device to use it
retinued pair list               print the waiting request, or "No device is waiting."
retinued pair confirm --tier T   approve the waiting request; T is viewer, chat or operator
retinued pair deny               refuse it
retinued pair cancel             throw the open code away
```

`retinued pair` prints, in this order: the URL, the code in its `K7QM-4XPD` form, the time it expires, and the exact command to approve a device that arrives. It never prints the console token, and the code appears once. Exit codes: `0` success, `1` the daemon refused (its sentence is printed), `2` usage error, `3` no console token (the daemon is not headless, or is not running).

---

# 3. Linux: what the daemon now does differently

| Area | Before | Now |
|---|---|---|
| Data folder | `%LOCALAPPDATA%\Retinue`, else the temp folder, on every OS | `RETINUE_DB_PATH` always wins. Otherwise Windows keeps `%LOCALAPPDATA%\Retinue\retinue.db`; Unix uses `$XDG_DATA_HOME/retinue/retinue.db`, else `~/.local/share/retinue/retinue.db`, else the temp folder (a host with no `HOME` is not a host to keep data on). |
| Secrets | Windows Credential Manager; every call on Linux returned "The credential store is only available on Windows." | Windows: unchanged. Unix: owner-only files, folder `secrets/` `0700`, one file per secret `0600`, written atomically (temp file, `sync`, `rename`, then `0600` again) in the shape of T3's `ServerSecretStore.set`. Names are hex-encoded so a name like `vault/<id>/password` cannot leave the folder. Never in the database, a backup, a diagnostics bundle or a log. |
| Secret Service keyring | — | **Not wired in this lane.** There is no Linux desktop app yet, so no user exists for it; the seam is named in `secrets.rs` and the report's open list rather than half-built. Headless Linux uses the files. |
| Tailscale | `tailscale.exe` on `PATH`, then `%ProgramFiles%\Tailscale` | `tailscale` on `PATH` off Windows, then `/usr/bin`, `/usr/local/bin`, `/snap/bin`, `/opt/homebrew/bin`. The "not installed" sentence names `tailscaled` on Unix. |
| Data-folder permissions | `icacls` on Windows; Unix branch already set `0700`/`0600` | Unchanged in behaviour; now the Doctor's answer on Linux is "owner only" when the folder is `0700` and owned by this user. |
| Pairing `platform` | hard-coded `"windows"` | `std::env::consts::OS` of the asking device. |
| Starting without a shell | prints `retinued.ready` and runs the gateway only if the owner turned Remote on | same, plus the console token file, so a headless host can be paired and managed from its terminal. |

## 3.1 Every `cfg(windows)` site in the daemon: what happens on Linux

The rule applied: **a Windows-only convenience is either given a Unix path, or refused at run time with a plain sentence, and never offered where it cannot work.** Counted from the daemon crate: 69 matches for `cfg(windows)`, `cfg(all(windows…))`, `target_os = "windows"` and `cfg(not(windows))` across `src/`, `src/remote/` and `tests/`.

### Had a Linux path already (read, unchanged)

| Site | What it is | On Linux |
|---|---|---|
| `main.rs:6226` | the parent watchdog (`WaitForSingleObject` on the shell's pid) | `getppid()` in a 250 ms loop |
| `sandbox.rs:26` / `:53` | restrict the data folder (`icacls`) | `set_permissions` `0700` (directory) / `0600` (file) |
| `sandbox.rs:90` / `:119` | the Doctor's "owner only" answer | reads the folder's mode bits |
| `remote/native.rs:382` / `:395` | the TLS key file | `chmod 0600` |
| `media.rs:936` / `:945` | "Show in folder" | `xdg-open <folder>` (no selection; a headless host has no viewer) |
| `diagnostics.rs:520` / `:524` | the OS version line | `uname -r` |
| `skills.rs:222` / `:228` | a reparse-point check beside `symlink_metadata` | `false`; the symlink test covers it |
| `preview.rs:344` / `:364`, `:371` / `:399` | the local-port scan (`netstat -ano`, and the pid table) | a stub that returns nothing: the preview panel loses its "which port is this" list and nothing else |
| `remote/connectivity.rs:513`, `:658` | `CREATE_NO_WINDOW` on the helper processes; the firewall | the flag is not needed; `add_rule` answers `unsupported` off Windows on its first line |
| `experiments.rs:18`, `project_setup.rs:236`, `project_clone.rs:247`, `repository.rs:256`, `repo_identity.rs:207`, `pull_request.rs:396`, `scaffold.rs:64`, `worktree.rs`, `worktree_attach.rs:78`, `worktree_cleanup.rs`, `remote/tailscale.rs:133` | `CREATE_NO_WINDOW`, so no console flashes when git or a CLI runs | nothing to set; the Unix branch simply does not add it |
| `project_setup.rs:256`/`:258`, `project_clone.rs:312`/`:318`, `repository.rs:392`/`:394` | `.cmd`/`.exe` resolution and `taskkill /T` | the crate's own Unix branch (extensions are `[""]` in `retinue-runtime`, and a process group is killed instead) |
| `secrets.rs:116` | the Windows Credential Manager backend | **replaced in this lane**: the owner-only file store (section 3) |
| `tests/parent_watchdog.rs`, `phase3_tests.rs:109`, `phase1b_tests.rs:621`, `retinue-process/tests/process_tree.rs:8` | a test with both branches | the Unix branch runs |

### Refused with a plain sentence, or not offered at all

| Site | What it is | On Linux |
|---|---|---|
| `owner_computer.rs` `mcp_server_for` | the owner's Windows computer's tools | **this lane:** not offered at all (`None`), so a bot never sees tools that cannot work |
| `owner_computer.rs` `mcp_rpc` | a call anyway (an old conversation, a stale bridge) | **this lane:** `mcp.owner.list` answers `{"tools": []}` and `mcp.owner.call` answers "The owner's computer is a Windows computer. This host cannot control one." |
| `remote/connectivity.rs` `add_rule` | the elevated firewall rule | `unsupported`; the guide says to use `ufw` and the router |
| `tests/live_cli.rs:1`, `tests/daemon_smoke.rs:1` | whole test files (`#![cfg(windows)]`) that start the real daemon and a real CLI | compile to nothing, so they neither run nor fail on Linux |
| `external_tests.rs:777`, `secrets_tests.rs:62`, `team_p10_tests.rs` (12 sites) | tests of Windows-only behaviour (a console flash, Credential Manager, a Windows path) | not compiled on Linux |

### Added in this lane

| Site | What it is |
|---|---|
| `paths.rs` | the XDG data-folder default, and `headless()` |
| `secrets.rs` `files` module | the owner-only file store, atomic write, hex file names |
| `remote/tailscale.rs` `exe_name`, `find_tailscale_in`, `missing_sentence` | the bare name on `PATH`, the distribution folders, the `tailscaled` sentence |
| `machines.rs` `platform()` | `std::env::consts::OS` instead of a hard-coded `windows` |
| `console.rs` | the headless console and its client |

### Known, not done (named rather than hidden)

- `crates/retinue-adapter-claude/src/lib.rs:50` picks the folder for a private instruction file from `LOCALAPPDATA`, then the temp folder. On Linux it lands in `/tmp/Retinue/private-tmp` with `0700` (the file itself is `0600`) instead of under the XDG data folder. Harmless, wrong place; it belongs to the adapter crate, not this lane.
- The preview port list (`/proc/net/tcp`) is not implemented; `04-linux-headless.md` calls it optional.


---

# 4. Install, update, stop

`docs/operations/linux-host.md` is the guide the owner or the Director runs: a systemd **user** unit (`~/.config/systemd/user/retinued.service`, `Restart=always`, `RestartSec=5`, `loginctl enable-linger`), the data folder, the secrets folder, environment variables, pairing, updating by replacing the binary and restarting, and the honest sentence about a turn in flight being lost on restart.

The unit file itself ships at `docs/operations/retinued.service`, so the guide and the file cannot drift.

---

# 5. Gates and what was run

Everything below was run in this worktree on the owner's PC on 17 November 2026, with `CARGO_TARGET_DIR` at `…\backend-work-from-the-owner-assigned\target`, `TEMP`/`TMP` at `C:\tmp`, and `RUST_MIN_STACK=33554432`.

| Gate | Result |
|---|---|
| `cargo check -p retinue-daemon --all-targets` | **passed**, no warnings. Under the documented Smart App Control workaround (`lessons.md`, 2026-11-16): `crates/retinue-daemon/build.rs` moved aside and the `include!(concat!(env!("OUT_DIR"), …))` line in `src/skill_library.rs` stubbed for the check, **both restored afterwards and verified byte-identical to the backup** — `git status` shows neither file changed. |
| `cargo clippy -p retinue-daemon --all-targets -- -D warnings` | **passed** after two test-only lints were fixed (`cloned_ref_to_slice_refs` in the new Tailscale test). |
| `cargo test -p retinue-daemon` | see "Tests" below: linked, **not executable** on this PC |
| `node scripts/check-architecture.mjs` | **passed** (`Architecture boundaries passed.`) |
| `node scripts/check-format.mjs` | **passed** (run after the stub was restored, so `build.rs` is present) |
| `node --test "scripts/**/*.test.mjs"` | **passed**, 33/33 |
| `cargo check --target x86_64-unknown-linux-gnu` | **blocked on this PC**: the target installs (`rustup target add x86_64-unknown-linux-gnu` succeeded), but the check stops at `libc`'s and `proc-macro2`'s build scripts with `An Application Control policy has blocked this file. (os error 4551)`, and `rusqlite` (`bundled`) and `rustls` (`ring`) would need a cross C toolchain in any case. **The `linux` CI job is the proof for Linux.** |
| `npm run typecheck`, `npm test`, `npm run docs:build` | **not run**: this worktree has no `node_modules`. The only JavaScript this lane touches is `tiers.json` (data) and `.github/workflows/ci.yml` (not read by Vite). **The Director must re-run `npm run docs:build`** because `docs/operations/linux-host.md` is a new page the library indexes (`scripts/docs-library.mjs` picks up every `docs/operations/*.md`) and `docs/operations/retinued.service` is not Markdown and is skipped. |
| tiers parity test (vitest) | could not run (no `node_modules`). Its two checks that this lane affects were replicated with `node` (`C:\tmp\lane-tier-check.cjs`, not committed): every `*.starts_with("…")` guard in `main_rpc` has a `tiers.json` family rule, and `console.*` is `never` with no tier. **Passed**; the real test should still run in CI. |

## Tests

- **Written:** `console_tests.rs` (6 tests), `paths.rs` (3 tests), `secrets.rs` `store::files` (5 tests, one Unix-only), `remote/tailscale.rs` (1 test), plus `PairingState::live_pending` exercised by the console tests.
- **Every `console.*` method is driven through `main_rpc`** with four credentials (none, a wrong one, the daemon capability, the console token), and the approval test reads the stored device row and the activity ledger afterwards. No new method is reachable only from its own module.
- **Run:** **not executable on this PC, and this is the known 17 November block, not a failure.** `cargo test -p retinue-daemon --bin retinued --no-run` linked `retinued-03c5e707c9ac5c7a.exe` in 10 m 20 s; running it — from the target folder, and copied to `C:\tmp` — was refused with `An Application Control policy has blocked this file` (os 4551, by content, not by path). `cargo clippy --all-targets` therefore **compiled** the test target with `-D warnings`, and the tests themselves run in the Windows `gates` job and the new `linux` job. What was proved here about the console tests: they compile, every method goes through `main_rpc`, and the assertions are written against `serde_json` values, not against the code's own helpers.

## What is left unverified, plainly

1. **No Linux binary has run.** Not on this PC (impossible), not on the VPS (no SSH from this lane). The first real run is the owner's or the Director's, following `docs/operations/linux-host.md`.
2. **The Secret Service keyring is not wired.** Off Windows, secrets are files. The amendment allows the keyring "on a Linux desktop with a running Secret Service keyring"; there is no Linux desktop app, so the seam is a comment and the open list, not code.
3. **`retinued pair` waiting loop and its printing are untested by a test.** The RPCs underneath them are tested; the terminal loop (2 s poll, 5-minute deadline) is not. A test would need a stub HTTP server; it is the first thing to add if the console changes.
4. **A real `tailscale serve` on Linux is unproven.** The discovery change is tested; the CLI's behaviour on a VPS (its `serve` syntax, its exit codes) is the same code path Windows already uses, but it has never been run there.
5. **The `linux` CI job has not run yet.** It is the first thing to look at on the next push.


---

# 6. Running log

Newest last. Each entry says what changed, what was run, and anything a later reader must know.

- **Report written first.** No code yet. Shapes above are the plan; if the code has to differ, the difference appears here and the shape is corrected above.

- **XDG data folder.** New `crates/retinue-daemon/src/paths.rs`: `database()`, `data_folder()`, `headless()`, and `default_db_path_from(lookup, windows, temp)` — the rule as a pure function, so both platforms' rules are tested on either. `main.rs` uses `paths::database()` where it had the `%LOCALAPPDATA%`-or-temp closure. Five tests in the module (Windows variable, XDG, `HOME` fallback, empty values, the folder being the database's parent). **`headless()` also reads `RETINUE_HEADLESS=1`**, which the unit sets, so "headless" is stated rather than inferred.

- **Secrets off Windows.** `secrets.rs` gained `store::files` (a module compiled on every platform so its tests run here): one hex-named file per secret under `<data>/secrets`, folder `0700`, file `0600`, written through a `create_new` temp file, `sync_all`, `rename`, then `0600` again — T3's `ServerSecretStore.set` shape (`apps/server/src/auth/ServerSecretStore.ts:193-228`, translated for the ideas only, no code copied). The non-Windows backend is five one-line wrappers over it, and `secrets::set_data_folder` is called once at startup. `store::backend_name()` answers `files` or `windows-credential-manager` for `console.status`. Five tests: round trip and replace, slashed names staying inside the folder and out of the listing, no temp file left, a bad name refused, and (on Unix only) `0700`/`0600`. **Difference from the amendment:** the Secret Service keyring is **not** wired — no Linux desktop app exists to use it yet; the seam is a comment and the report's open list. Say so in review.

- **Tailscale on `PATH`.** `exe_name()` is `tailscale` off Windows; `find_tailscale_in(path, exe, defaults)` is the rule, with the distribution folders (`/usr/bin`, `/usr/local/bin`, `/snap/bin`, `/opt/homebrew/bin`, `/opt/local/bin`) as the fallback and `%ProgramFiles%\Tailscale` kept on Windows. The "not installed" sentence now names `tailscaled` on Unix. One test with a temporary folder proves PATH-first, the fallback, and `None` when nothing is found.

- **The pairing `platform`.** `machines.rs` `platform()` is `std::env::consts::OS`; the two hard-coded `"windows"` bodies are gone. Nothing else in the request shape changed.

- **Owner's computer refused off Windows.** `owner_computer::mcp_server_for` returns `None` (so the tools are never offered) and `mcp_rpc` answers `{"tools": []}` or one plain sentence. This is a behaviour change on Linux only; Windows is untouched.

- **The console.** New `console.rs` and `console_tests.rs`. Six methods (`console.status`, `console.remote.enable`, `console.remote.disable`, `console.pair.begin`, `console.pair.pending`, `console.pair.confirm`, `console.pair.deny`, `console.pair.cancel` — eight names, six decisions) routed in `main_rpc` before the capability check, with a token of their own; `<data>/console.json` is written at a headless start and removed at a clean exit. `PairingState::live_pending()` is the new read (no id needed, expired requests excluded). The CLI is `retinued pair …`, `retinued remote on|off`, `retinued status`, `retinued --version`. `tiers.json` gained the `console.*` family row as `never`. Six tests, every one through `main_rpc`.

- **Linux CI job.** `.github/workflows/ci.yml` gained a `linux` job (ubuntu-latest): `cargo check -p retinue-daemon --all-targets`, `cargo test -p retinue-daemon --no-fail-fast`, `cargo test -p retinue-process`, `cargo clippy -p retinue-daemon --all-targets -- -D warnings`, and `cargo build -p retinue-daemon --bin retinued`. It installs `build-essential`, `pkg-config` and `cmake` first, because `rusqlite` is `bundled` and `rustls` uses `ring`. Both Windows jobs are untouched.

- **The unit and the guide.** `docs/operations/retinued.service` (a user unit: `RETINUE_HEADLESS=1`, a `PATH` line, `Restart=always`, `OOMPolicy=continue`, `KillMode=mixed`, `MemoryLow`, OOM preference, log file) and `docs/operations/linux-host.md` (install, linger, where data and secrets live, turning the gateway on, pairing both methods, updating, stopping, what is not available on Linux, troubleshooting).

- **Gates run** (details in section 5): `cargo check -p retinue-daemon --all-targets` clean, `cargo clippy -p retinue-daemon --all-targets -- -D warnings` clean (after two test-only lints), `node scripts/check-architecture.mjs` and `node scripts/check-format.mjs` pass, `node --test "scripts/**/*.test.mjs"` 33/33. The test binary links but **cannot run on this PC** (Smart App Control, os 4551, by content). The Linux cross-check is blocked the same way; the CI `linux` job is the proof.

- **Shape change (1 of 1 so far).** `console.status` sends the **whole `remote.status` object** under `remote`, not the four-field subset this report first described: a subset means every field `remote.status` gains must be copied here by hand, and the Director's screens already read that object. Section 2.1 is corrected. `retinued status` prints the version, folder, secrets store, Remote state, any Remote error, and the Native certificate fingerprint (the guide tells the owner to compare it).

---

# 7. Files touched

**New:** `crates/retinue-daemon/src/paths.rs`, `crates/retinue-daemon/src/console.rs`, `crates/retinue-daemon/src/console_tests.rs`, `docs/operations/linux-host.md`, `docs/operations/retinued.service`, this report.

**Changed:** `crates/retinue-daemon/src/main.rs` (data-folder default, `mod paths`, `mod console`, `AppState.console` and its five initialisers, the `console.` route in `main_rpc`, `console::startup`/`shutdown`, the CLI and `--version`, the `console_tests` include), `crates/retinue-daemon/src/secrets.rs` (the file store, `set_data_folder`, `backend_name`), `crates/retinue-daemon/src/remote/tailscale.rs` (discovery, the Unix sentence, one test), `crates/retinue-daemon/src/remote/pairing.rs` (`live_pending`), `crates/retinue-daemon/src/machines.rs` (`platform()`), `crates/retinue-daemon/src/owner_computer.rs` (refused off Windows), `crates/retinue-daemon/src/sandbox.rs` (the Doctor's sentence), `apps/desktop/src/remote/tiers.json` (`console.*`, `never`), `scripts/check-architecture.mjs` (the `console.rs` network allowlist row), `.github/workflows/ci.yml` (the `linux` job), `docs/remote/THREAT_MODEL.md` (section 16's headless-Linux rows), `docs/reference/invariants.md` (two rows), and the development skill: `references/lessons.md` (a 2026-11-17 entry), `references/codebase-map.md` (the headless-host entry), `references/verification.md` (a Linux section, in both copies of the file — the file itself is duplicated in the repository, which is not this lane's to fix).

**Line endings:** every new file is LF; `tiers.json`, `ci.yml`, `main.rs`, `secrets.rs` and the skill files were edited in place and kept their CRLF.

## Owner and Director decisions this lane owes

1. **The Secret Service keyring** is not wired (section 5). Either accept files on Linux for now, or make the keyring a follow-up lane when a Linux desktop app exists.
2. **The console token file** is a new credential on disk. The threat model's new row says what it is worth; the owner may want to say no to it and keep only an in-process console, which would mean the console cannot be used while the daemon runs under systemd.
3. **`npm run docs:build`** must be re-run for `docs/operations/linux-host.md` to appear in the documentation library.


