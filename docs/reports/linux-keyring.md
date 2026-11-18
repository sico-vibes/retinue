# Lane report: Linux secrets in the system keyring

Lane: Ada (backend). Worktree `backend-task-from-the-director-claude-17` (the main checkout), cut from `d96a738`. Date 18 November 2026.
Owner decision: 18 November 2026, "on Linux, use the system keyring for Retinue's secrets". The wording it completes is the 17 November "Headless Linux daemon for the VPS" bullet ("on a Linux desktop with a running Secret Service keyring, the keyring is used") and the seam `docs/reports/multi-device-linux.md` left open.
Read first: `docs/reports/multi-device-linux.md` (the file store and the seam), `crates/retinue-daemon/src/secrets.rs`, `docs/operations/linux-host.md`, `docs/remote/THREAT_MODEL.md` §16, `docs/reference/invariants.md`.

**This file is the contract first, then the running log at the end.** Where the code differs from this contract, the difference appears in the log and the contract above is corrected in place.

---

# 1. The contract

| Question | Answer |
|---|---|
| Do the RPCs change? | **No.** `secret.*`, `secret.request.*`, the vault, connectors, channels, Pi, the linked computer and webhooks all call the same `store::read`/`write`/`delete`/`list`, and no reply gains a field. |
| Does an existing reply change meaning? | One value: `console.status`'s `secrets.backend` is now `"secret-service"` on a Linux host whose keyring is running and unlocked, where it was always `"files"`. It is the only new string, and `retinued status` prints it unchanged. |
| New dependency | `secret-service = "=5.2.0"`, `default-features = false`, `features = ["rt-async-io-crypto-rust"]`, under `[target.'cfg(not(windows))'.dependencies]` only. Pure Rust over D-Bus (zbus), no `libdbus` headers, no OpenSSL. Licence and version: `docs/legal/third-party-notices.md`, "System keyring on Linux". |
| New events, settings, bot tools | None. |
| The Doctor | One new row: id `secrets.store`, area `safety`, status `ok` on all three stores, and a plain sentence for each (`Credential Manager`, `system keyring`, `owner-only files`). No new area, no fix. |
| Where the values live | Windows: unchanged, Credential Manager, `Retinue/<name>`. Off Windows: one item per secret in the default collection of the user's Secret Service, attributes `application=retinue` and `name=<name>`, label `Retinue: <name>`; or the owner-only files `<data>/secrets` (folder `0700`, file `0600`) when no keyring is usable. |
| When the keyring is used | Only when the startup probe finds a Secret Service on the user's session bus **and** its default collection reports unlocked **and** the file store's migration moved every value it held. Otherwise files. Chosen once, at startup, and never re-probed during the run. |
| A locked keyring | Files, and one `info` line in the log at startup. Nothing ever calls `unlock`, so no unit is left waiting on a prompt. |
| A host with no session bus | Files, silently, with no retry loop: `zbus::Connection::session()` fails at once, and every keyring call runs on its own thread with a **2-second** cap (`store::keyring::WAIT`). |
| Migration | Each stored value (internal names such as `vault/<id>/password`, `connector/<id>` and `pi/<provider>` included) is written to the keyring, read back and compared, and only then is its file deleted. A value that does not move keeps that run on the files, so it never becomes invisible. A failed copy leaves the file and the next start tries again. |
| Logging | One `info` line when a locked keyring is found, one when values moved, one `warn` when a value did not move. Names are never logged, values never. |
| Remote reach | Unchanged. No `secret.*` method became reachable, and `vault.*` stays `never`. |

## 1.1 The rule as code

- `store::choose(found: Keyring, migration: &Migration) -> Choice` — `Keyring` is `Unlocked`, `Locked` or `Absent`; `Choice` is `Files` or `Keyring`.
- `store::migrate(from: &dyn Backend, to: &dyn Backend) -> Migration` — `{moved, kept, failed}`, names only.
- `Backend` is the four-method trait both stores implement; `entries()` returns the internal names `secret.list` hides, which is why the file store gained `all_in` beside `list_in`.

## 1.2 What a Linux desktop or host now does, by state

| State at startup | Store used | What the owner sees |
|---|---|---|
| Windows | Credential Manager | `console.status` says `windows-credential-manager` |
| Linux, keyring running and unlocked, files empty or fully moved | keyring | `secret-service`; moved values logged once |
| Linux, keyring running and unlocked, a value did not move | files | `files`; a `warn` line, value still in the files |
| Linux, keyring locked | files | `files`; an `info` line once, no prompt |
| Linux, no session bus (a normal headless VPS) | files | `files`; nothing logged, no delay |

`docs/reports/multi-device-linux.md` §3 says "Secret Service keyring — not wired in this lane"; that is the 17 November history, and this lane supersedes the line, not the report.

---

# 2. Gates and what was run

Everything below ran in this worktree on the owner's PC on 18 November 2026, with `CARGO_TARGET_DIR=…\backend-work-from-the-owner-assigned-3\target`, `TEMP`/`TMP` at `C:\tmp`, `RUST_MIN_STACK=33554432`.

| Gate | Result |
|---|---|
| `cargo check -p retinue-daemon --all-targets` | **passed**, no warnings (2m 21s). The Windows build never compiles the keyring module (`cfg(not(windows))`). |
| `cargo clippy -p retinue-daemon --all-targets -- -D warnings` | **passed** (1m 44s), which also typechecks the new pure-function tests. |
| `node scripts/check-format.mjs` | **passed** (1508 files). |
| `node scripts/check-architecture.mjs` | **passed**. No new allowlist row is needed: the keyring talks D-Bus, not HTTP, so the `daemon-network` rule does not apply. |
| `cargo test -p retinue-daemon --bin retinued --no-run` | linked `retinued-bb4d7e650180a8d1.exe` (6m 15s). |
| Running the test binary | The copy in `C:\tmp` was **refused by Smart App Control** (os 4551); the binary in `…\target\debug\deps\` ran. Targeted results below. |
| `secrets::store::` (9 tests) | **9 passed**: the five new rule/migration tests, the four file-store tests. |
| `doctor::tests::` (4) | **4 passed**, including `doctor_run_returns_the_documented_shape_and_names_no_secret` with the new row. |
| `console_status_reports_…` | **1 passed** (reads `backend_name()`). |
| `there_is_no_way_to_read_a_value_back` | **1 passed** (the listing, after the `all_in`/`list_in` split). |
| `fix39_p12_tests::every_log_call_site_is_a_fixed_message` | **1 passed**, run from `crates/retinue-daemon` (it reads sources relative to the crate folder): the three new log lines use fixed wording with none of the words the scan forbids. |
| Linux build | **not possible on this PC** (cross-target build scripts are refused by Smart App Control, and `rusqlite`/`ring` need a cross C toolchain). **The `linux` CI job is the proof**, as `references/verification.md` says. |
| `npm run docs:build` | **not run**: this worktree has no `node_modules`. The CI `docs` job runs `npm run docs:check`, so the Director must run `npm run docs:build` for the four changed pages (`linux-host.md`, `THREAT_MODEL.md`, `invariants.md`, the notices) and the new report to refresh the library. |

## 2.1 What is left unverified, plainly

1. **The keyring module has never been compiled.** It is `cfg(not(windows))`, so neither `cargo check` nor clippy on this PC sees it. It was written against `secret-service` 5.2.0's own source in the local registry (the blocking `connect`/`get_default_collection`/`create_item`/`search_items`/`get_all_items`/`get_secret`/`get_attributes`/`get_modified`/`delete` signatures were read, not guessed). A throwaway probe crate at `C:\tmp\keyring-probe` was written to typecheck it on Windows; it stopped at `getrandom 0.4.3`'s build script (SAC, os 4551), before reaching the module. **The first real compile is the `linux` CI job.**
2. **No real keyring has been exercised.** The Linux-only test (`keyring::tests::the_real_keyring_holds_a_value_when_this_computer_has_one`) skips unless a session bus and an unlocked collection are present; a CI runner has neither, so it will report "skipping". On a desktop it writes, reads, lists and deletes an item named `keyring-test-<uuid>`.
3. **A real GNOME keyring's attribute search and label** are unproven. The code sets `application=retinue`, `name=<name>` and the label `Retinue: <name>`, and finds items by the two attributes through the default collection's `search_items`.
4. **The 2-second cap** is a wall-clock wait on the calling thread; a D-Bus call that outlives it leaves its worker thread to finish on its own. It was chosen so a unit with no session bus never waits, not measured against a hung keyring.
5. **A host that has migrated and then starts with a locked keyring** uses the files, which are now empty, so a bot whose secret points at a keyring item fails with "missing from the credential store" rather than "the store is unavailable". That follows the owner's rule for a locked keyring (files, never a prompt); the guide's troubleshooting table tells the owner to unlock and restart. Worth a sentence in the UI copy if a real desktop hits it.
6. **`AGENTS.md` in this worktree does not contain the 17 November amendment** ("Headless Linux daemon for the VPS"); its newest amendment is 16 November. The rule was taken from the task's wording and from `THREAT_MODEL.md` §16 and `docs/operations/linux-host.md`, which both record it. `AGENTS.md` is the Director's and was not edited.

---

# 3. Files touched

**New:** `docs/reports/linux-keyring.md` (this file).

**Changed:** `crates/retinue-daemon/src/secrets.rs` (the `Backend` trait, `choose`, `migrate`, the keyring module, the startup choice, the file store's `all_in`, the five new tests), `crates/retinue-daemon/src/doctor.rs` (one row), `crates/retinue-daemon/Cargo.toml` (the dependency), `Cargo.lock` (the resolved tree: `secret-service` 5.2.0 and the newer crypto crate versions it needs; zbus 5.19.0 was already there through Tauri), `docs/operations/linux-host.md`, `docs/remote/THREAT_MODEL.md` §16 (one row, two corrections), `docs/reference/invariants.md` (the off-Windows secrets row), `docs/legal/third-party-notices.md`, and the development skill (`references/codebase-map.md`, `references/lessons.md`).

**Line endings:** every edited file kept CRLF; the new report is LF like the other reports.

---

# 4. Running log

Newest last.

- **Contract written, then the dependency.** `secret-service` was pinned exactly (`=5.2.0`) rather than by caret, so a Linux build cannot silently move to a new API; it is the same family `keyring`'s `sync-secret-service` wraps, but it exposes the attributes and label this task requires, which `keyring`'s `Entry` API does not. `cargo fetch` resolved it and rewrote `Cargo.lock` (LF; converted back to CRLF at the end of the lane).

- **The rule and the migration as pure functions.** `Backend`, `Keyring`, `Choice`, `Migration`, `choose` and `migrate` sit above the stores and are compiled on every platform under `test`, so five tests run on Windows: the rule's three cases, a full move (internal names included), a refused write, a copy that reads back differently, and an unreadable source. Each asserts the value is still in the old store when anything fails.

- **The file store's two listings.** `name_of` no longer filters on `valid_name`; `list_in` filters, and the new `all_in` does not. Without that, the migration would have skipped `vault/<id>/password`, `connector/<id>` and `pi/<provider>` — every internal name — and those values would have become invisible the moment the keyring was chosen. A test now asserts `all_in` sees the slashed name `list_in` hides.

- **The keyring module.** Blocking client on a worker thread (the crate warns that its blocking API must not run in a runtime context), 2-second `recv_timeout` on every call, `is_locked` only — never `unlock` — and `create_item(..., replace: true, "text/plain")`. Values are held in `Zeroizing` between the caller and the crate.

- **Choosing once, at startup.** `secrets::set_data_folder` (already the one startup hook) now calls `store::startup()` off Windows: probe, migrate, choose, log, and store the choice in a `OnceLock`. Before startup, and in test builds, the choice is `Files`.

- **The Doctor.** One `safety` row naming the store in words. The Doctor's own test forbids `token`, `password` and `api_key` in a detail, so the row names "system keyring" and "owner-only files" rather than the raw backend string; `console.status` and `retinued status` carry the raw name.

- **Gates run** (details in section 2): check, clippy, format, architecture, the test binary built, and 16 targeted tests passed (the five new rule/migration tests, the four file-store tests, four Doctor tests, the console status read, the listing test, and the log-site scan). The copy to `C:\tmp` was refused by Smart App Control while the binary in `deps\` ran; that is the known behaviour, not a test failure.
