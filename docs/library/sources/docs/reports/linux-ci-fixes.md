# Lane report: the daemon's first full run on Linux (CI job `linux`, commit `79d9ae1`)

Lane: Ada (backend). Worktree `linux-ci-fixes-from-the-director`, branch `retinue/linux-ci-fixes-from-the-director`, at `79d9ae1` (`git fetch origin && git merge --ff-only origin/main`). Date 18 November 2026.

Request: the new `linux` CI job (ubuntu-latest) compiled the daemon and ran its whole suite for the first time: **1,298 passed, 13 failed**, and three Linux-only warnings made `cargo clippy -D warnings` fail. Fix them so the daemon is correct on Linux (the owner's VPS runs it headless) without changing Windows behaviour.

**Classification, in one line each:** 3 real Linux gaps fixed in code (handoff path redaction, the Native host name, and duplicate machine reads — a concurrency race that Linux's scheduling exposed), 9 tests that assumed Windows fixed to use the platform's own paths and shell, and 1 test of a Windows-only feature made to assert the by-design refusal on Linux. The three warnings are `cfg` fixes. Windows behaviour is unchanged: every `cfg` branch keeps today's Windows value.

---

## 1. The 13 failures

| # | Test | Verdict | Cause | Fix |
|---|---|---|---|---|
| 1 | `machines_tests::a_change_emits_machine_changed` | **code** (Linux exposed a race; not Linux-specific in principle) | The background loop and the test's own read both called `machines::refresh_one` for one link. Both read the same old row, both wrote "online" and both emitted `machine.changed`, so the test counted a third event. The same overlap lets an older read put its snapshot over a newer one. | `Machines.refreshes`: one `tokio::sync::Mutex` per link, held across a read (`refresh_one` → `refresh_one_locked`). Removed with the link. `machines.rs`. |
| 2 | `machines_tests::a_native_host_name_is_matched_without_regard_to_case` | **code** | `remote::host_name()` read only `COMPUTERNAME`/`HOSTNAME`. The Linux CI test process has neither, so it answered "This computer"; `is_dns_name` drops that from the certificate SANs, and `Host: this computer:<port>` is refused with 403. A systemd unit also sets no `HOSTNAME`, so the VPS certificate would carry no host name. | On Unix the system's own host name (`gethostname`) is read first, then `HOSTNAME`; Windows keeps `COMPUTERNAME` then `HOSTNAME`, unchanged. `remote/mod.rs`; `gethostname = "1"` under `[target.'cfg(unix)'.dependencies]` (already in `Cargo.lock` via `x11rb`). |
| 3 | `machines_tests::the_package_has_no_paths_or_thinking_or_tool_output` | **code** | `handoff::has_host_path` knew a drive letter, a UNC share and four Unix home folders. A notice naming `/tmp/retinue-machines-package-repos-…/giver-retinue/fix-export` was neither, so it travelled in the handoff package. | An absolute Unix token (starts with `/`, longer than one character, not `//host`) now counts as a host path. The unit test gained `/tmp`, `/srv`, `/opt` and quoted cases, and `and / or`, relative `a/b` and a URL as non-paths. `handoff.rs`. |
| 4 | `phase2a_tests::a_resolved_file_must_stay_inside_the_project_folder` | **test** | The fixture spelled `C:\work\shop…`; on Linux that is one file name, not a tree, so `stays_inside` could not match. | The same five cases written in each platform's own shape through `if cfg!(windows)` (both branches are typechecked on either OS). `project_icon_tests.rs`. |
| 5 | `phase2a_tests::issue_post_needs_confirm_and_redacts_again` | **test** | The `gh` stub records `%*` on Windows (quotes kept) and `"$@"` on Unix (no quotes), so `--title "Crash on start"` only matched on Windows. The daemon passes argv directly on both platforms; nothing is shell-parsed on Unix. | The test strips the quote character, as `pull_request.rs` already does, and asserts the same argv words in the same order. `phase1b_tests.rs`. |
| 6 | `phase2a_tests::outside_sessions_are_opt_in_reversible_and_keep_no_content` | **test** | The hook's `cwd` was the Windows path `C:\work\site`. `outside::hook` keeps `Path::file_name()`, which on Linux sees the whole string as one name. The rule is right for a real Linux `cwd`. | `cwd` is `/work/site` on Linux (`if cfg!(windows)`), and the "the full path never appears" assertion uses the JSON-escaped path the reply actually carries. `main.rs`. |
| 7 | `phase2a_tests::setup_script_failure_does_not_stop_the_turn_and_tells_the_agent` | **test** | The script was `echo … 1>&2 & exit /b 3`. The daemon runs `sh -c` off Windows, and `sh` read `/b3` as a number for `exit`: `sh: 1: exit: Illegal number: /bENOSPC while installing`. | Each platform's own shell: `… 1>&2 & exit /b 3` on Windows, `… 1>&2; exit 3` on Unix. Same stderr line, same non-zero exit, same assertion. `phase1_tests.rs`. |
| 8 | `phase2a_tests::setup_script_timeout_is_reported_and_does_not_stop_the_turn` | **test** | `ping -n 30 127.0.0.1 > nul` is not a program on Linux, so the script failed at once and the notice said `failed`, not `timeout`. | `ping … > nul` on Windows, `sleep 30` on Unix; the 400 ms test limit and the timeout assertion are unchanged. `phase1_tests.rs`. |
| 9 | `phase2a_tests::stopping_a_turn_stops_its_setup_script` | **test** | `cmd /c "ping … & echo late> late.txt"` is not a program on Linux, so the script failed before the stop and there were two notices, not one. | `cmd /c "…"` on Windows, `sleep 5; echo late > late.txt` on Unix: `sh` starts `sleep` as its own child, and the process-group kill that the test proves is the same. Assertions unchanged. `phase1_tests.rs`. |
| 10 | `phase2a_tests::submodules_are_initialised_in_the_worktree` | **test** (transport, not daemon logic) | In a linked worktree `git submodule update` clones into `.git/worktrees/<id>/modules`, and that clone reads only environment config, never the superproject's `protocol.file.allow=always`. The local-path submodule origin therefore fails with `fatal: transport 'file' not allowed` and `Failed to clone 'sub' a second time, aborting`. Reproduced on Windows with git 2.55; the Windows CI runner's git reuses the main module and never clones. | A test-only, scoped allowance for the length of that one test: `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=protocol.file.allow`, `GIT_CONFIG_VALUE_0=always`, which the daemon's git child inherits (`LocalSubmoduleOrigins` in `phase3_tests.rs`). The daemon still refuses the file protocol in production. |
| 11 | `phase2a_tests::the_server_is_not_offered_at_all_while_the_switch_is_off` | **test** (the feature is Windows-only by design) | `owner_computer::mcp_server_for` returns `None` off Windows on purpose (17 November amendment), and the test asserted the Windows answer. | `if cfg!(windows)` keeps the existing endpoint assertions; the Linux branch asserts the refusal: no server with the switch on, `mcp.owner.list` empty, `mcp.owner.call` says "The owner's computer is a Windows computer. This host cannot control one.", and no endpoint file is written. `owner_computer_tests.rs`. |
| 12 | `project_clone::tests::urls_are_checked_before_git_runs` | **test** | The "test-only local repository" was `C:/local/repo`; `Path::is_absolute()` is false on Linux, so the allowance was never reached. | The local path is `C:/local/repo` on Windows and `/tmp/local/repo` on Unix; the same two assertions (refused for a real call, allowed only through the allowance). `project_clone.rs`. |
| 13 | `project_share::tests::a_path_inside_the_base_is_the_only_one_accepted` | **test** | The `..` cases were built with a literal `\`, which is an ordinary character in a Linux file name, so the path never resolved and `confined` answered "That folder is not on this computer." instead of the outside-folder sentence. | `std::path::MAIN_SEPARATOR` in the two `format!` calls. The `ParentDir` refusal is then the one exercised, on both platforms. `project_share.rs`. |

## 2. The three warnings (`-D warnings`)

| Location | Warning | Fix |
|---|---|---|
| `experiments.rs:17` | `unused_mut`: `let mut command` on Linux, where the only mutation is the Windows `creation_flags` | The binding is `mut` only in the `#[cfg(windows)]` arm (a second `let` for the other platform). |
| `preview.rs:48` | `dead_code`: `NETSTAT_TIMEOUT` used only in the Windows `netstat` | `#[cfg(windows)]` on the constant. |
| `preview.rs:50` | `dead_code`: `NETSTAT_MAX_BYTES`, same | `#[cfg(windows)]` on the constant. |

## 3. What was run (on the owner's PC)

- `cargo check -p retinue-daemon --all-targets` — **passed** (borrowed target `backend-work-from-the-owner-assigned\target`, the real `build.rs`, nothing stubbed).
- `cargo clippy -p retinue-daemon --all-targets -- -D warnings` — **passed** (1 m 31 s), which is the gate the Linux job runs.
- `cargo test -p retinue-daemon --bin retinued --no-run` — linked `retinued-8b1395adcdc29356.exe` (4 m 16 s); Smart App Control refused the first binary (os 4551, by content), the **relink was allowed** (49 s) and the copy at `C:\tmp\ada-linux-ci2.exe` ran. A later formatting-only edit forced one more relink, which was allowed too (`C:\tmp\ada-linux-ci3.exe`), and the same 13 tests were re-run on it.
- Every one of the 13 tests named above ran and **passed** on the final binary, each by `--exact` from `crates/retinue-daemon` with `TEMP`/`TMP` at `C:\tmp` (the owner-computer test's real name is `phase2a_tests::…`, because the file is `include!`d).
- The touched modules' suites also ran: `machines_tests::` **98 passed, 1 failed** (`a_device_clone_is_confined_and_reports_its_progress_and_a_failure`, the desktop job's status read too early); it **passes alone**, a known load-flaky race, and is not one of the 13. Also green: `project_share::tests::`, `project_clone::tests::`, `phase2a_tests::issue_` (4), `phase2a_tests::outside_`, `phase2a_tests::setup_script_` (4), `handoff::tests::a_notice_with_a_host_path_is_recognised`.
- `node scripts/check-format.mjs` — **passed** (1,508 files); `node scripts/check-architecture.mjs` — **passed**.
- The two tests added by section 6 ran on the final binary too, and the 13 above were re-run with them: **16 passed, 0 failed**.

## 4. What is left unverified, plainly

1. **No Linux binary has run.** Nothing on this PC can build for Linux (see the skill's `verification.md`, "Linux"). **The CI `linux` job is the proof.**
2. **The `cfg(unix)` branches are not typechecked by a Windows clippy run.** Two are tiny (`gethostname` in `host_name`, the dependency line); every other platform branch is written with `if cfg!(windows)`, so Windows *does* typecheck the Linux side. The two Unix strings in `phase1_tests` are plain literals.
3. **The submodule allowance is the one place CI's git version matters.** It was reproduced and fixed on git 2.55 (Windows); the Ubuntu runner's git should honour `GIT_CONFIG_*` the same way, and the next `linux` run proves it.
4. **`tests/live_cli.rs` and `tests/daemon_smoke.rs`** are `#![cfg(windows)]` and do not compile on Linux by design; they are not in this diff.

## 5. Files touched

**Changed:** `crates/retinue-daemon/Cargo.toml`, `Cargo.lock` (the `gethostname` row), `crates/retinue-daemon/src/experiments.rs`, `handoff.rs`, `machines.rs`, `main.rs`, `owner_computer_tests.rs`, `phase1_tests.rs`, `phase1b_tests.rs`, `phase3_tests.rs`, `preview.rs`, `project_clone.rs`, `project_icon_tests.rs`, `project_share.rs`, `remote/mod.rs`, `remote/tailscale.rs`, `crates/retinue-daemon/tests/daemon_smoke.rs`, `tests/live_cli.rs`, `scripts/perf-gate.mjs`, `scripts/live-checks/live-composer.mjs`, `scripts/live-checks/live-plan.mjs`, `scripts/live-checks/live-team.mjs`, `docs/operations/retinued.service`, `docs/operations/linux-host.md`, `docs/remote/THREAT_MODEL.md`, `docs/ipc-contract.md`; `crates/retinue-daemon/src/machines_tests.rs` (sections 6-7); the development skill's `references/lessons.md` and `references/verification.md`; this report.

**New:** `docs/reports/linux-ci-fixes.md`.

**Line endings:** every edited file was edited in place and kept its existing CRLF working-tree endings (`Cargo.lock` was rewritten by cargo with LF and converted back to CRLF); the new report is CRLF like the existing reports. In the index, git normalises them to LF as it does for the rest of the tree.

**Never committed** (owner's rule for lanes): the work sits in this worktree with no `git commit`.

---

# 6. The VPS install's two findings (second request, 18 November 2026)

Source: `docs/reports/vps-install-2026-11-18.md` in `operations-task-from-the-director-claude`, findings 1 and 2.

## 6.1 The capability reached a log file on a headless host

- **Cause:** `daemon_main` printed `retinued.ready` with `capability` on stdout. The systemd unit appends stdout to `~/.local/share/retinue/retinued.log` (`0644` inside a `0700` folder), so the one secret that opens every daemon method sat at rest there. `AGENTS.md` (17 November) and `docs/remote/THREAT_MODEL.md` both forbid it.
- **Fix (code):** a new `ready_line(headless, address, capability)` in `main.rs`, called with `paths::headless()`. On a headless host the line is `type`, `protocolVersion` and `address` only. A daemon the desktop shell started is serialised from `DaemonReady` exactly as before, capability included, so the Tauri shell's `ReadyLine` is untouched.
- **Test:** `phase2a_tests::a_headless_host_keeps_the_capability_off_stdout` — both shapes, and the secret value appears nowhere in the headless line. The integration test `tests/daemon_smoke.rs` (Windows-only) now passes `RETINUE_PARENT_PID`, because it starts the real binary as a shell would; without it the binary is headless and no longer prints a capability it needs.
- **Every other reader of the line, and what changed:** the Tauri shell already passes its own pid (unchanged); `tests/live_cli.rs` (Windows-only), `scripts/perf-gate.mjs` (CI's `perf` job, which reads the capability for `app.snapshot`) and the three `scripts/live-checks/*.mjs` now pass `RETINUE_PARENT_PID` too. Without that they would be headless, see no capability, and every RPC they make would be `unauthorized` — the perf gate would fail CI.
- **Unit:** `docs/operations/retinued.service` sets `UMask=0077`, so the log, the database and its `-wal` are `0600`.
- **Existing hosts:** the guide's update section now says to clear an old `retinued.log` once (`: > ~/.local/share/retinue/retinued.log`), because a beta.5 log still holds a dead capability at rest.
- **Threat model:** a row in section 16's headless Linux table, naming the line, the switch, the console token as the reason nothing there needs the capability, and the test.
- **No API change:** `DaemonReady` and the shell's `ReadyLine` keep their shape. Only the headless line omits the field, and nothing on a headless host reads it.

## 6.2 The Tailscale refusal did not name its fix

- **Cause:** `remote::enable` answered any refused `tailscale serve` with "Tailscale refused to publish the Retinue address." The CLI's own reason (`Access denied: serve config denied`, which names the operator command) went nowhere on purpose, because Retinue never prints CLI output.
- **Fix:** `tailscale::serve_refusal(&CliOutput)`. When the output says both `access denied` and `serve config denied`, the fixed sentence is: "Tailscale refused to publish the Retinue address. Run `sudo tailscale set --operator=$USER` once, then try again." Any other refusal keeps the old sentence. No output, host name, path or argv travels.
- **Test:** `remote/tailscale.rs::a_serve_refusal_names_the_operator_fix_and_never_echoes_tailscale` — the installed CLI's exact text in, the command out, none of the raw text out; another refusal keeps the plain sentence.
- **Guide:** `docs/operations/linux-host.md` section 2 gains the Arch one-time `sudo systemctl start "user@$(id -u).service"` after `enable-linger` (the exact symptom from the install: "Failed to connect to user scope bus via local transport"), and section 3 says the unit's `UMask=0077` makes the database and log `0600`.
- **Merge note for the Director:** the operations worktree already adds the section 4 `sudo tailscale set --operator="$USER"` paragraph and a troubleshooting row for the old sentence. After this change the message itself names the command, so that row's second sentence ("Tailscale's own reason is printed only when you run `tailscale serve` by hand") is no longer true, and should be dropped when the two worktrees are merged. This lane did not touch section 4 or the troubleshooting table, so that merge stays clean.

## 6.3 What was run for section 6, and what is not proven

- Windows: `cargo clippy -p retinue-daemon --all-targets -- -D warnings` — **clean**; the two new tests and the previous 13 — **16 passed, 0 failed** on the relinked binary.
- `node scripts/check-format.mjs` — **passed**; `node scripts/check-architecture.mjs` — **passed**; `node --test "scripts/**/*.test.mjs"` — **52 passed, 0 failed**; `node --check` on the four changed scripts — clean.
- **The real startup path could not be run on this PC:** `cargo build` linked `retinued.exe` and `perf-seed.exe`, and Smart App Control refused both (`An Application Control policy has blocked this file`, by content), so neither the daemon's ready line nor the perf gate could be exercised end to end here. The ready line is covered by its function test; the perf gate's env addition is one line identical to the shell's own and the script tests pass. CI's `perf` and `linux` jobs are the proof.
- **Not proven here:** no Linux binary has run. The unit file is text: `UMask=0077` and the Arch `user@` step follow systemd's documented behaviour but were not executed on the host by this lane. The refusal sentence was exercised only through its unit test with Tailscale 1.104's recorded output, not against a live `tailscaled`.
- **Documentation library:** `docs/ipc-contract.md` now states the headless bootstrap shape. The library mirrors under `docs/library/` are generated, and this worktree has no `node_modules`; **the Director should run `npm run docs:build`** with the merge (the operations worktree already owes one for its new guide text).

---

# 7. Two more Linux failures: the tier tests (third request, 18 November 2026)

`machines_tests::a_raised_tier_reaches_the_link` and `a_lowered_tier_reaches_the_link` failed on Linux at `machines_tests.rs:1278`, "an unchanged tier emits nothing" (left 4, right 3). Both go through one helper, `tier_follows`.

- **Cause, and the evidence:** the daemon emits `machine.changed` from **two asynchronous sources**: a read (`machines::refresh_one`, which compares the row before and after) and the live socket (`machine_stream::announce`, on connect and on a drop). The tier helper sampled a baseline right after one read and compared it after the next, with no wait for the socket. A temporary trace on Windows showed the intended order for `a_raised_tier`: read (version/kind fill), stream connect, read (tier rise), then the stream's disconnect only after the test disabled Remote. On a host where the socket connects later, its own event lands between the baselines, and the next read is blamed for it. This is **the same cause as `a_change_emits_machine_changed`** (which already waits for the socket), not a field that differs on Linux: the diagnostic showed every compared field (`state`, `detail`, `version`, `kind`, `bots`, `tier`) unchanged between the tier reads.
- **Fix (test, with the assertion kept exact):** a new `quiet_changed_count` waits until no `machine.changed` has been written for 100 ms (bounded at 20 s), and the helper waits for `crate::machine_stream::live` to be true before the baseline and again after the tier change (so a socket that was dropped and reconnected has announced itself). The next read must still add **no** event: `assert_eq!(changed_count(&me), settled, "an unchanged tier emits nothing")`. `a_change_emits_machine_changed` now takes its `after_first` baseline through the same helper.
- **Verified on Windows:** `a_raised_tier_reaches_the_link`, `a_lowered_tier_reaches_the_link` and `a_change_emits_machine_changed` pass; the whole `machines_tests::` suite — **99 passed, 0 failed** (including the one that had been load-flaky). `cargo clippy -p retinue-daemon --all-targets -- -D warnings` — clean.
- **Not proven here:** the Linux run itself. The reasoning is that the trace's third read is the only read-caused event; if the next Linux run still fails, the remaining suspicion is a transient failed read turning the link offline and back, which would need a daemon fix rather than a test one.
